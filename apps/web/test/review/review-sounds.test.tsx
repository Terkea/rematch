// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameReviewView } from "../../src/app/(app)/app/games/[gameId]/review-view";
import { toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

const played = vi.hoisted(() => ({ calls: [] as string[], muted: false }));

vi.mock("@ci/ui/audio", async (orig) => {
  const real = await orig<typeof import("@ci/ui/audio")>();
  return {
    ...real,
    useBoardSounds: () => {
      const [muted, setMuted] = useState(played.muted);
      return {
        play: (s: string) => {
          if (!muted) played.calls.push(s);
        },
        muted,
        setMuted: (m: boolean) => {
          played.muted = m;
          setMuted(m);
        },
        volume: 0.6,
        setVolume: () => {},
      };
    },
  };
});

vi.mock("next/navigation", () => ({
  usePathname: () => "/app/games/x",
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
}));

// 1.e4 e5 2.Nf3 d5 3.exd5 Qxd5 4.Bb5+  (a capture and a check)
const SANS = ["e4", "e5", "Nf3", "d5", "exd5", "Qxd5", "Bb5+"];
const view = () => toReviewView(game(SANS, {}, {}));

beforeEach(() => {
  played.calls = [];
  played.muted = false;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView ??= () => {};
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: true,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }),
  });
});
afterEach(() => vi.unstubAllGlobals());

const key = (k: string) => act(() => void fireEvent.keyDown(window, { key: k }));

describe("review sounds", () => {
  it("plays the sound of the move stepped onto, one per step", () => {
    render(<GameReviewView view={view()} initialPly="0" />);
    key("ArrowRight");
    expect(played.calls).toEqual(["move"]);
    key("ArrowRight");
    key("ArrowRight");
    key("ArrowRight");
    key("ArrowRight");
    expect(played.calls).toEqual(["move", "move", "move", "move", "capture"]);
  });

  it("plays the undone move's sound when stepping back", () => {
    render(<GameReviewView view={view()} initialPly="5" />);
    key("ArrowLeft");
    expect(played.calls).toEqual(["capture"]);
  });

  it("plays nothing at the ends, where a step does not move", () => {
    render(<GameReviewView view={view()} initialPly="0" />);
    key("ArrowLeft");
    expect(played.calls).toEqual([]);
  });

  it("plays at most one sound when jumping several moves", () => {
    render(<GameReviewView view={view()} initialPly="0" />);
    key("End");
    expect(played.calls).toEqual(["check"]);
  });

  it("has a visible, labelled sound switch that mutes the board", () => {
    render(<GameReviewView view={view()} initialPly="0" />);
    const sw = screen.getByRole("switch", { name: /board sounds/i });
    expect(sw).toHaveAttribute("aria-checked", "true");
    fireEvent.click(sw);
    expect(sw).toHaveAttribute("aria-checked", "false");
    key("ArrowRight");
    expect(played.calls).toEqual([]);
  });

  it("sounds a held key's first step, then at most one per 120 ms, and nothing on release", () => {
    let t = 10_000;
    vi.spyOn(performance, "now").mockImplementation(() => t);
    render(<GameReviewView view={view()} initialPly="0" />);
    const down = (repeat: boolean) =>
      act(() => void fireEvent.keyDown(window, { key: "ArrowRight", repeat }));
    down(false);
    expect(played.calls).toHaveLength(1);
    for (let i = 0; i < 6; i++) {
      t += 30; // OS key repeat
      down(true);
    }
    // 180 ms of repeats: only the one that lands 120 ms after the first sounds
    expect(played.calls).toHaveLength(2);
    t += 200;
    act(() => void fireEvent.keyUp(window, { key: "ArrowRight" }));
    expect(played.calls).toHaveLength(2);
    vi.restoreAllMocks();
  });
});
