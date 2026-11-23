// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
}));

const { OverviewEmpty } = await import("../src/app/(app)/app/_overview/empty-state.js");
const { NoLinks } = await import("../src/app/(app)/app/games/_explorer/empty.js");
const { RevealPanel } = await import("../src/app/(app)/app/train/train-session.js");

// The Rematch brand's hand-made touches (DESIGN.md, "Hand-made touches"): dad's margin note sits
// only in empty states and the trainer's solved moment, never on data, and it never replaces the
// words a screen reader needs.

const notes = (c: HTMLElement) => [...c.querySelectorAll("[data-margin-note]")];

describe("margin notes in empty states", () => {
  it("the overview with no account linked has one, hidden from assistive tech", () => {
    const { container } = render(<OverviewEmpty state={{ kind: "no-links" }} />);
    const [note, ...rest] = notes(container);
    expect(rest).toHaveLength(0);
    expect(note?.getAttribute("aria-hidden")).toBe("true");
    // The real sentence is still there for everyone.
    expect(container.textContent).toContain("Connect a chess.com account to see your games here.");
  });

  it("the games list with no account linked has one", () => {
    const { container } = render(<NoLinks />);
    expect(notes(container)).toHaveLength(1);
  });
});

const reveal = (correct: boolean) => ({
  correct,
  recorded: true,
  feedback: correct ? "Right: Rd8# mates." : "Not this time: Rd8# mates.",
  move: correct ? { uci: "d1d8", san: "Rd8#" } : { uci: "d1d2", san: "Rd2" },
  solution: { uci: "d1d8", san: "Rd8#" },
  line: [],
  played: { uci: "d1d2", san: "Rd2" },
  game: {
    id: "g1",
    ply: 41,
    opponent: "",
    playedAt: "2026-11-20T18:00:00Z",
    href: "/app/games/g1",
  },
  sourceKey: "blunder" as never,
  dueOn: null,
  retired: false,
});

describe("the trainer's solved moment", () => {
  const panel = (correct: boolean) =>
    render(
      <RevealPanel
        result={reveal(correct)}
        moveNumber={21}
        colour="white"
        source={null}
        last={false}
        nextRef={createRef()}
        onNext={() => {}}
      />,
    ).container;

  it("a solved drill gets a celebrating margin note, decorative to a screen reader", () => {
    const c = panel(true);
    const [note] = notes(c);
    expect(note?.getAttribute("aria-hidden")).toBe("true");
    expect(note?.querySelector("[data-crayon-stroke]")).not.toBeNull();
    // The verdict keeps its words and its status role.
    expect(c.querySelector("[role='status']")?.textContent).toContain("Right: Rd8# mates.");
  });

  it("a missed drill gets none: no crayon on a mistake", () => {
    expect(notes(panel(false))).toHaveLength(0);
  });
});
