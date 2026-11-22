// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { renderToString } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SEEN_KEY } from "../../src/app/(app)/app/games/[gameId]/reveal/seen-store";
import { GameReviewView } from "../../src/app/(app)/app/games/[gameId]/review-view";
import { type ReviewView, toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

// M5a T11: the analysis reveal (DESIGN.md Choreography §1, R10). One rAF clock, so the tests fake
// requestAnimationFrame and performance with the timers.

vi.mock("next/navigation", () => ({
  usePathname: () => "/app/games/00000000-0000-4000-8000-000000000001",
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
}));

const ID = "00000000-0000-4000-8000-000000000001";
// The Two Knights, 16 plies (synthetic marks, B15).
const SANS = "e4 e5 Nf3 Nc6 Bc4 Nf6 Ng5 d5 exd5 Na5 Bb5+ c6 dxc6 bxc6 Be2 h6".split(" ");

function analysed(): ReviewView {
  return toReviewView({
    ...game(SANS, {
      3: { winWhite: 45, judgement: "mistake", deltaW: 12, bestUci: "d2d4" },
      5: { winWhite: 35, judgement: "blunder", deltaW: 20, bestUci: "f1b5" },
      9: { winWhite: 20, judgement: "blunder", deltaW: 30, bestUci: "d2d3" },
      16: { winWhite: 20 },
    }),
    // Worst first by deltaW (T3), so the climax plays 3, then 5, then 9.
    moments: [9, 5, 3],
  });
}

function media({ reduced, wide = true }: { reduced: boolean; wide?: boolean }) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: query.includes("prefers-reduced-motion") ? reduced : wide,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: [
      "setTimeout",
      "clearTimeout",
      "setInterval",
      "clearInterval",
      "requestAnimationFrame",
      "cancelAnimationFrame",
      "performance",
      "Date",
    ],
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView ??= () => {};
  localStorage.clear();
  media({ reduced: false });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const root = () => document.querySelector("[data-review]") as HTMLElement;
const shownPly = () => Number(root().getAttribute("data-ply"));
const shownFen = () => root().getAttribute("data-fen");
const phase = () => root().getAttribute("data-reveal");
const mode = () => root().getAttribute("data-reveal-mode");
const animated = () => root().getAttribute("data-animate-annotations");
const marks = () => document.querySelector("[data-annotations]");
const at = (ms: number) => act(() => vi.advanceTimersByTime(ms));
const status = () => screen.getByTestId("step-status").textContent;

function mount(view = analysed(), initialPly?: string) {
  return render(<GameReviewView view={view} gameId={ID} {...(initialPly ? { initialPly } : {})} />);
}

describe("the analysis reveal, first view", () => {
  it("plays the third-worst moment first and ends on the worst with its marks", () => {
    const view = analysed();
    mount(view);
    expect(phase()).toBe("running");
    expect(mode()).toBe("full");
    at(1200 + 160);
    // The position BEFORE the third-worst move (ply 3), so ply 2.
    expect(shownPly()).toBe(2);
    expect(shownFen()).toBe(view.plies[1]?.fenAfter);
    at(7220 - 1360);
    expect(shownPly()).toBe(9);
    expect(shownFen()).toBe(view.plies[8]?.fenAfter);
    expect(animated()).toBe("true");
    expect(marks()?.querySelectorAll("[data-mark]").length).toBeGreaterThan(0);
    at(40);
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(9);
  });

  it("plays each beat: before the move, then the move, then its marks, then retracts", () => {
    mount();
    at(1200 + 300); // the hold on the position before ply 3
    expect(shownPly()).toBe(2);
    expect(marks()).toBeNull();
    at(300); // 1800: the move was played from t0 + 400
    expect(shownPly()).toBe(3);
    expect(marks()).toBeNull();
    at(400); // 2200: the marks draw from t0 + 860
    expect(marks()).not.toBeNull();
    at(950); // 3150: retracted after t0 + 1900
    expect(marks()).toBeNull();
    expect(shownPly()).toBe(3);
    at(210); // 3360: the second beat, before ply 5
    expect(shownPly()).toBe(4);
  });

  it("announces Analysis ready under StrictMode too, and nothing on mount", () => {
    render(
      <StrictMode>
        <GameReviewView view={analysed()} gameId={ID} />
      </StrictMode>,
    );
    at(1000);
    expect(status()).toBe("");
    at(6400);
    expect(status()).toBe("Analysis ready");
  });

  it("runs the full version under StrictMode's double effects (seen is decided once)", () => {
    render(
      <StrictMode>
        <GameReviewView view={analysed()} gameId={ID} />
      </StrictMode>,
    );
    expect(mode()).toBe("full");
    at(1360);
    expect(shownPly()).toBe(2);
  });

  it("marks the game seen when the reveal starts", () => {
    mount();
    expect(JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]")).toEqual([ID]);
  });

  it("jumps to the end state on any key", () => {
    mount();
    at(2000);
    expect(shownPly()).toBe(3);
    fireEvent.keyDown(window, { key: "Shift" });
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(9);
    expect(animated()).toBe("true");
    expect(marks()).not.toBeNull();
  });

  it("does not swallow the key that skips: ArrowRight skips and steps", () => {
    mount();
    at(2000);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(10);
  });

  it("jumps to the end state on a pointer press, and the press still acts", () => {
    mount();
    at(2000);
    const row = document.querySelector('[aria-label="Where it went wrong"] [data-ply="5"]');
    fireEvent.pointerDown(row as Element);
    expect(phase()).toBe("done");
    fireEvent.click(row as Element);
    expect(shownPly()).toBe(5);
  });

  it("shows Skip while running, which ends the reveal, then Replay analysis", () => {
    mount();
    at(500);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(phase()).toBe("done");
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
    expect(screen.getByRole("button", { name: "Replay analysis" })).toBeTruthy();
  });

  it("replays the full version from Replay analysis", () => {
    localStorage.setItem(SEEN_KEY, JSON.stringify([ID]));
    mount();
    expect(mode()).toBe("condensed");
    at(700);
    expect(phase()).toBe("done");
    fireEvent.click(screen.getByRole("button", { name: "Replay analysis" }));
    expect(phase()).toBe("running");
    expect(mode()).toBe("full");
    at(1360);
    expect(shownPly()).toBe(2);
  });

  it("announces Analysis ready once, politely, when it ends", () => {
    mount();
    expect(status()).toBe("");
    at(7300);
    expect(screen.getByTestId("step-status").getAttribute("aria-live")).toBe("polite");
    expect(status()).toBe("Analysis ready");
    // Nothing overwrites it on its own (the shown move did not change).
    at(1000);
    expect(status()).toBe("Analysis ready");
    // A step replaces it; a replay's end does not announce again.
    fireEvent.keyDown(window, { key: "ArrowRight" });
    at(400);
    expect(status()).not.toBe("Analysis ready");
    fireEvent.click(screen.getByRole("button", { name: "Replay analysis" }));
    at(7300);
    expect(phase()).toBe("done");
    expect(status()).not.toBe("Analysis ready");
  });
});

describe("later views and exits", () => {
  it("runs condensed on a second mount of the same game (600ms, on the worst moment)", () => {
    const first = mount();
    at(7300);
    first.unmount();
    mount();
    expect(phase()).toBe("running");
    expect(mode()).toBe("condensed");
    expect(shownPly()).toBe(9);
    expect(animated()).toBe("true");
    expect(marks()).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
    at(580);
    expect(phase()).toBe("running");
    at(40);
    expect(phase()).toBe("done");
  });

  it("shows the end state at t = 0 under reduced motion, and marks nothing seen", () => {
    media({ reduced: true });
    mount();
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(9);
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Replay analysis" })).toBeNull();
    expect(localStorage.getItem(SEEN_KEY)).toBeNull();
  });

  it("starts nothing when hydrating under reduced motion (the server assumed full motion)", () => {
    const tree = <GameReviewView view={analysed()} gameId={ID} />;
    const host = document.createElement("div");
    host.innerHTML = renderToString(tree);
    document.body.append(host);
    media({ reduced: true });
    let root: ReturnType<typeof hydrateRoot> | undefined;
    act(() => {
      root = hydrateRoot(host, tree);
    });
    try {
      at(50);
      expect(phase()).toBe("done");
      expect(mode()).toBeNull();
      expect(localStorage.getItem(SEEN_KEY)).toBeNull();
      expect(status()).toBe("");
    } finally {
      act(() => root?.unmount());
      host.remove();
    }
  });

  it("skips the reveal for ?ply=12 and shows ply 12", () => {
    mount(analysed(), "12");
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(12);
    at(2000);
    expect(shownPly()).toBe(12);
    expect(localStorage.getItem(SEEN_KEY)).toBeNull();
  });

  it("never reveals a game that is still waiting for analysis", () => {
    const waiting = toReviewView({ ...game(SANS), analysed: false, analysisStatus: "pending" });
    mount(waiting);
    expect(phase()).toBe("done");
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Replay analysis" })).toBeNull();
    expect(screen.getByTestId("waiting").textContent).toMatch(/waiting for analysis/);
    expect(localStorage.getItem(SEEN_KEY)).toBeNull();
  });

  it("runs condensed, never the full version in a loop, when storage throws", () => {
    vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
      throw new Error("SecurityError");
    });
    mount();
    expect(mode()).toBe("condensed");
  });

  it("ends on the final position after the 600ms graph draw when nothing is marked", () => {
    const clean = toReviewView({ ...game(SANS), moments: [] });
    mount(clean);
    expect(mode()).toBe("full");
    at(620);
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(SANS.length);
  });
});

// Fix round 1 (review-t11.md).
const slot = (where: "board" | "phone") =>
  document.querySelector(`[data-reveal-controls="${where}"]`);

describe("layout stability (I1, I2)", () => {
  it("reserves both control slots in the server HTML, before any run has started", () => {
    const html = renderToString(<GameReviewView view={analysed()} gameId={ID} />);
    expect(html).toContain('data-reveal-controls="board"');
    expect(html).toContain('data-reveal-controls="phone"');
  });

  it("keeps the slots, empty, under reduced motion, so nothing moves when it is turned off", () => {
    media({ reduced: true });
    mount();
    expect(slot("board")).not.toBeNull();
    expect(slot("board")?.querySelector("button")).toBeNull();
  });

  it("has no slots for a game without analysis (nothing to reveal or replay)", () => {
    mount(toReviewView({ ...game(SANS), analysed: false, analysisStatus: "pending" }));
    expect(slot("board")).toBeNull();
    expect(slot("phone")).toBeNull();
  });

  it("swaps Skip for Replay inside the same board slot from 768px", () => {
    mount();
    const box = slot("board");
    expect(box?.textContent).toBe("Skip");
    at(7300);
    expect(slot("board")).toBe(box);
    expect(box?.textContent).toBe("Replay analysis");
    expect(slot("phone")?.querySelector("button")).toBeNull();
  });

  it("puts the control in the move line's slot on a phone, adding no row above the board", () => {
    media({ reduced: false, wide: false });
    mount();
    expect(slot("phone")?.textContent).toBe("Skip");
    expect(slot("board")?.querySelector("button")).toBeNull();
    at(7300);
    expect(slot("phone")?.textContent).toBe("Replay analysis");
  });

  it("names the green arrow only once a beat's marks are drawn, in a reserved line", () => {
    mount();
    const line = () => screen.getByTestId("arrow-key-slot");
    at(1200 + 600); // ply 3 played, marks not yet drawn
    expect(shownPly()).toBe(3);
    expect(line()).toBeTruthy();
    expect(screen.queryByTestId("arrow-key")).toBeNull();
    at(400); // t0 + 1000: marks drawing
    expect(screen.getByTestId("arrow-key").textContent).toMatch(/^Green arrow: d4/);
    at(950); // retracted
    expect(screen.queryByTestId("arrow-key")).toBeNull();
    expect(line()).toBeTruthy();
  });
});

describe("storage and clock edges (m1, m2)", () => {
  it("runs condensed when the store reads but cannot write, so it never loops the full version", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceeded");
    });
    mount();
    expect(mode()).toBe("condensed");
  });

  it("does not restart the clock when the view object changes mid-run", () => {
    const r = mount();
    at(2000);
    expect(shownPly()).toBe(3);
    r.rerender(<GameReviewView view={analysed()} gameId={ID} />);
    at(10);
    expect(shownPly()).toBe(3);
    at(7300 - 2010);
    expect(phase()).toBe("done");
    r.rerender(<GameReviewView view={analysed()} gameId={ID} />);
    expect(phase()).toBe("done");
    expect(shownPly()).toBe(9);
  });
});
