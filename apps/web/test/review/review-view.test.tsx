// @vitest-environment jsdom
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GameReviewView } from "../../src/app/(app)/app/games/[gameId]/review-view";
import { toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

vi.mock("next/navigation", () => ({
  usePathname: () => "/app/games/00000000-0000-4000-8000-000000000001",
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
  useSearchParams: () => new URLSearchParams(),
}));

const SANS = ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5"];
const MATE_HIGH = 97.54474363414323;

function analysed(more: Parameters<typeof game>[2] = {}) {
  return toReviewView({
    ...game(
      SANS,
      {
        1: {
          winWhite: 52,
          score: { cp: 20 },
          scoreSource: "engine",
          bestUci: "e2e4",
          classification: "best",
        },
        3: { winWhite: 40, judgement: "mistake", deltaW: 12, bestUci: "d2d4" },
        5: { winWhite: 30, judgement: "blunder", deltaW: 10, bestUci: "d2d4" },
        6: { winWhite: 30, classification: "best", bestUci: "g8f6" },
        7: {
          winWhite: 90,
          score: { cp: 600 },
          judgement: "blunder",
          deltaW: MATE_HIGH - 90,
          bestUci: "c4f7",
        },
        8: { winWhite: 90 },
      },
      more,
    ),
    moments: [3, 7, 5],
  });
}

function wide(matches: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches,
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
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView ??= () => {};
  wide(true);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const sheet = () => screen.getByRole("region", { name: /^Moves,/ });
const current = () =>
  within(sheet())
    .getAllByRole("button")
    .find((b) => b.getAttribute("aria-current") === "step")
    ?.getAttribute("data-ply");

describe("GameReviewView", () => {
  it("steps one ply on ArrowRight and reports it", () => {
    const onStep = vi.fn();
    render(<GameReviewView view={analysed()} initialPly="2" onStep={onStep} />);
    expect(current()).toBe("2");
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(current()).toBe("3");
    expect(onStep).toHaveBeenLastCalledWith("step", 3);
  });

  it("moves one ply, not two, when the graph slider has focus", () => {
    render(<GameReviewView view={analysed()} initialPly="2" />);
    const slider = screen.getByRole("slider");
    slider.focus();
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(current()).toBe("3");
  });

  it("goes to the last ply on End, as a jump", () => {
    const onStep = vi.fn();
    render(<GameReviewView view={analysed()} initialPly="2" onStep={onStep} />);
    fireEvent.keyDown(window, { key: "End" });
    expect(current()).toBe(String(SANS.length));
    expect(onStep).toHaveBeenLastCalledWith("jump", SANS.length);
  });

  it("reports play ticks every second", () => {
    vi.useFakeTimers();
    const onStep = vi.fn();
    render(<GameReviewView view={analysed()} initialPly="2" onStep={onStep} />);
    fireEvent.click(screen.getAllByRole("button", { name: "Autoplay" })[0] as HTMLElement);
    act(() => vi.advanceTimersByTime(1000));
    expect(onStep).toHaveBeenLastCalledWith("play", 3);
  });

  it("mirrors the ply to ?ply= after 150ms", () => {
    vi.useFakeTimers();
    const replace = vi.spyOn(window.history, "replaceState");
    render(<GameReviewView view={analysed()} initialPly="2" />);
    replace.mockClear();
    fireEvent.keyDown(window, { key: "ArrowRight" });
    fireEvent.keyDown(window, { key: "ArrowRight" });
    act(() => vi.advanceTimersByTime(149));
    expect(replace).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1));
    expect(replace).toHaveBeenCalledTimes(1);
    expect(String(replace.mock.calls[0]?.[2])).toContain("ply=4");
  });

  it("shows no clock text for a game without clocks", () => {
    const { container } = render(<GameReviewView view={analysed()} initialPly="2" />);
    expect(container.textContent).not.toMatch(/\d+:\d\d/);
  });

  it("shows the clocks when the game has them", () => {
    const clocks = [598, 597, 590, 585, 580, 575, 570, 560];
    const { container } = render(<GameReviewView view={analysed({ clocks })} initialPly="2" />);
    expect(container.textContent).toMatch(/9:5\d/);
  });

  it("shows the waiting copy and no graph for a pending game", () => {
    const view = toReviewView({ ...game(SANS), analysed: false, analysisStatus: "pending" });
    render(<GameReviewView view={view} />);
    expect(
      screen.getByText("This game is waiting for analysis. You can replay the moves now."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("slider")).toBeNull();
    expect(screen.queryByRole("meter")).toBeNull();
    // The moves still replay.
    expect(current()).toBe(String(SANS.length));
  });

  it("says when analysis failed or the game was skipped", () => {
    const base = { ...game(SANS), analysed: false };
    const { unmount } = render(
      <GameReviewView view={toReviewView({ ...base, analysisStatus: "failed" })} />,
    );
    expect(screen.getByText("Analysis failed for this game.")).toBeInTheDocument();
    unmount();
    render(<GameReviewView view={toReviewView({ ...base, analysisStatus: "skipped" })} />);
    expect(
      screen.getByText("This game was skipped: it is a variant or too short to analyse."),
    ).toBeInTheDocument();
  });

  it("prefixes a derived score with ≈ and leaves an engine score bare", () => {
    const { unmount } = render(<GameReviewView view={analysed()} initialPly="3" />);
    expect(screen.getByTestId("eval-readout").textContent).toMatch(/^≈ /);
    unmount();
    render(<GameReviewView view={analysed()} initialPly="1" />);
    expect(screen.getByTestId("eval-readout").textContent).toBe("+0.2");
  });

  it("gives the shown move its verdict sentence", () => {
    render(<GameReviewView view={analysed()} initialPly="7" />);
    expect(screen.getByTestId("verdict").textContent).toBe(
      "4. Ng5?? is a blunder: it let a forced mate go. Your win chance 98% to 90%. The engine preferred Bxf7+.",
    );
  });

  it("lists where it went wrong in game order, with the measure each was judged on", () => {
    render(<GameReviewView view={analysed()} initialPly="2" />);
    const ledger = screen.getByRole("list", { name: "Where it went wrong" });
    const rows = within(ledger).getAllByRole("button");
    expect(rows.map((r) => r.getAttribute("data-ply"))).toEqual(["3", "5", "7"]);
    expect(rows[2]?.textContent).toContain("98% to 90%");
    expect(rows[2]?.textContent).toContain("marked for losing a forced mate");
    // Severity: only a blunder is in full pen.
    expect(rows[0]?.querySelector("[data-severity]")?.getAttribute("data-severity")).toBe(
      "mistake",
    );
    expect(rows[1]?.querySelector("[data-severity]")?.getAttribute("data-severity")).toBe(
      "blunder",
    );
    fireEvent.click(rows[1] as HTMLElement);
    expect(current()).toBe("5");
  });

  it("collapses a long ledger to the three worst, with a way to show all", () => {
    const many = [
      "e4",
      "e5",
      "Nf3",
      "Nc6",
      "Nc3",
      "Nf6",
      "Bb5",
      "Bb4",
      "O-O",
      "O-O",
      "d3",
      "d6",
      "Bg5",
      "Bg4",
      "Nd5",
      "Nd4",
      "Nxf6+",
      "gxf6",
    ];
    // The player's (White's) odd plies 1..15 are inaccuracies costing 5..12 points.
    const marks: Parameters<typeof game>[1] = {};
    for (let i = 0; i < 8; i++)
      marks[2 * i + 1] = { winWhite: 50 - i, judgement: "inaccuracy", deltaW: 5 + i };
    const eight = toReviewView({
      ...game(many.slice(0, 16), marks),
      moments: [15, 13, 11, 9, 7, 5, 3, 1],
    });
    const list = () => screen.getByRole("list", { name: "Where it went wrong" });
    const { unmount } = render(<GameReviewView view={eight} initialPly="2" />);
    expect(within(list()).getAllByRole("button")).toHaveLength(8);
    expect(screen.queryByRole("button", { name: /^Show all/ })).toBeNull();
    unmount();
    const nine = toReviewView({
      ...game(many, { ...marks, 17: { winWhite: 30, judgement: "blunder", deltaW: 20 } }),
      moments: [17, 15, 13, 11, 9, 7, 5, 3, 1],
    });
    render(<GameReviewView view={nine} initialPly="2" />);
    // The three worst, in game order: 13 (11 points), 15 (12), 17 (20).
    expect(
      within(list())
        .getAllByRole("button")
        .map((r) => r.getAttribute("data-ply")),
    ).toEqual(["13", "15", "17"]);
    // The shown move stays listed when it is one of the mistakes.
    fireEvent.keyDown(window, { key: "Home" });
    fireEvent.click(
      within(screen.getByRole("group", { name: "Move controls" })).getByRole("button", {
        name: "Next mistake",
      }),
    );
    expect(
      within(list())
        .getAllByRole("button")
        .map((r) => r.getAttribute("data-ply")),
    ).toEqual(["1", "13", "15", "17"]);
    fireEvent.click(screen.getByRole("button", { name: "Show all 9 marked moves" }));
    expect(within(list()).getAllByRole("button")).toHaveLength(9);
  });

  it("ranks a ?? above any ? when collapsing, whatever its drop", () => {
    const many = [
      "e4",
      "e5",
      "Nf3",
      "Nc6",
      "Nc3",
      "Nf6",
      "Bb5",
      "Bb4",
      "O-O",
      "O-O",
      "d3",
      "d6",
      "Bg5",
      "Bg4",
      "Nd5",
      "Nd4",
      "Nxf6+",
      "gxf6",
    ];
    const marks: Parameters<typeof game>[1] = {};
    for (let i = 0; i < 8; i++)
      marks[2 * i + 1] = { winWhite: 50 - i, judgement: "inaccuracy", deltaW: 5 + i };
    // A forced mate lost for a 3-point drop, and a plain blunder.
    marks[9] = { winWhite: 46, judgement: "blunder", deltaW: 3 };
    marks[17] = { winWhite: 30, judgement: "blunder", deltaW: 20 };
    const view = toReviewView({
      ...game(many, marks),
      moments: [17, 15, 13, 11, 7, 5, 3, 9, 1],
    });
    render(<GameReviewView view={view} initialPly="2" />);
    const list = screen.getByRole("list", { name: "Where it went wrong" });
    expect(
      within(list)
        .getAllByRole("button")
        .map((r) => r.getAttribute("data-ply")),
    ).toEqual(["9", "15", "17"]);
  });

  it("puts the thumb bar's mistake jumps where they can be read", () => {
    wide(false);
    render(<GameReviewView view={analysed()} initialPly="2" />);
    const bar = screen.getByRole("group", { name: "Move controls, thumb bar" });
    fireEvent.click(within(bar).getByRole("button", { name: "Next mistake" }));
    expect(screen.getByTestId("verdict").textContent).toMatch(/^2\. Nf3\?/);
  });

  it("labels both ends of the phone eval bar", () => {
    wide(false);
    render(<GameReviewView view={analysed()} initialPly="2" />);
    const meter = screen.getByRole("meter");
    expect(meter.textContent).toContain("White");
    expect(meter.textContent).toContain("Black");
  });

  // Fix round 1 (critic 3): the always-on green arrow is named, so an off switch doesn't
  // read as broken; the switch says it shows the next move.
  it("names the green arrow beside the verdict when one is drawn", () => {
    const { unmount } = render(<GameReviewView view={analysed()} initialPly="3" />);
    expect(screen.getByTestId("arrow-key").textContent).toBe(
      "Green arrow: d4, the engine's choice instead.",
    );
    unmount();
    render(<GameReviewView view={analysed()} initialPly="2" />);
    expect(screen.queryByTestId("arrow-key")).toBeNull();
  });

  // Fix round 1 (review-t10 I2, I3): one page-level announcer, present at every width.
  it("announces a step on a phone, once the steps settle", () => {
    vi.useFakeTimers();
    wide(false);
    render(<GameReviewView view={analysed()} initialPly="1" />);
    const status = screen.getByTestId("step-status");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status.textContent).toBe("");
    fireEvent.keyDown(window, { key: "ArrowRight" });
    fireEvent.keyDown(window, { key: "ArrowRight" });
    act(() => vi.advanceTimersByTime(299));
    expect(status.textContent).toBe("");
    act(() => vi.advanceTimersByTime(1));
    expect(status.textContent).toMatch(/^2\. Nf3\? is a mistake/);
  });

  it("says nothing while autoplaying, then names the move it stops on", () => {
    vi.useFakeTimers();
    wide(false);
    render(<GameReviewView view={analysed()} initialPly="1" />);
    const status = screen.getByTestId("step-status");
    const bar = screen.getByRole("group", { name: "Move controls, thumb bar" });
    fireEvent.click(within(bar).getByRole("button", { name: "Autoplay" }));
    act(() => vi.advanceTimersByTime(4500));
    expect(status.textContent).toBe("");
    fireEvent.click(within(bar).getByRole("button", { name: "Autoplay" }));
    act(() => vi.advanceTimersByTime(300));
    expect(status.textContent).toMatch(/^3\. Bc4\?\?/);
  });

  // M5a T13: "N s on this move" is the mover's, for White's and Black's moves, both ways up.
  it("puts the spent line with the player who moved, not the side to move", () => {
    const clocks = [598, 595, 590, 580, 575, 570, 560, 550];
    for (const playerColor of ["white", "black"] as const) {
      for (const [ply, mover] of [
        [3, "Whitey"],
        [4, "Blacky"],
      ] as const) {
        const { unmount } = render(
          <GameReviewView view={analysed({ playerColor, clocks })} initialPly={String(ply)} />,
        );
        const line = document.querySelector("[data-spent-for]");
        expect(line?.textContent, `${playerColor} ply ${ply}`).toMatch(new RegExp(`^${mover}: `));
        // The mover's row is the one just before it; the board's bottom is the player's colour.
        const row = line?.previousElementSibling;
        expect(row?.textContent, `${playerColor} ply ${ply}`).toContain(mover);
        unmount();
      }
    }
  });

  // M5a T13 (T10 re-review m2): the first paint reads nothing and writes no ?ply=, at either width.
  it("reads nothing and leaves the URL alone on the first paint", () => {
    for (const isWide of [true, false]) {
      vi.useFakeTimers();
      wide(isWide);
      const replace = vi.spyOn(window.history, "replaceState");
      const { unmount } = render(<GameReviewView view={analysed()} initialPly="3" />);
      act(() => vi.advanceTimersByTime(2000));
      expect(screen.getByTestId("step-status").textContent, `wide ${isWide}`).toBe("");
      expect(replace, `wide ${isWide}`).not.toHaveBeenCalled();
      replace.mockRestore();
      unmount();
    }
  });

  // T10 re-review m3: a move is read again when the player comes back to it after another was
  // read; a step away and back inside the 300 ms settle reads nothing (nothing changed).
  it("re-reads a move the player returns to, but not a step away and back", () => {
    vi.useFakeTimers();
    wide(false);
    render(<GameReviewView view={analysed()} initialPly="1" />);
    const status = screen.getByTestId("step-status");
    const step = (key: string) => fireEvent.keyDown(window, { key });
    step("ArrowRight");
    act(() => vi.advanceTimersByTime(300));
    const second = status.textContent;
    expect(second).toMatch(/^1… e5/);
    step("ArrowRight");
    step("ArrowLeft");
    act(() => vi.advanceTimersByTime(1000));
    expect(status.textContent).toBe(second);
    step("ArrowRight");
    act(() => vi.advanceTimersByTime(300));
    expect(status.textContent).not.toBe(second);
    step("ArrowLeft");
    act(() => vi.advanceTimersByTime(300));
    expect(status.textContent).toBe(second);
  });

  it("keeps the scoresheet's own announcer quiet, so a step is read once", () => {
    vi.useFakeTimers();
    render(<GameReviewView view={analysed()} initialPly="1" />);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    act(() => vi.advanceTimersByTime(400));
    const statuses = screen.getAllByRole("status").filter((s) => s.textContent !== "");
    expect(statuses).toHaveLength(1);
  });

  it("tags Best on the player's own moves only", () => {
    render(<GameReviewView view={analysed()} initialPly="2" />);
    expect(document.getElementById("ply-1")).toHaveTextContent("Best");
    expect(document.getElementById("ply-6")).not.toHaveTextContent("Best");
  });

  it("toggles the engine move with its switch", () => {
    render(<GameReviewView view={analysed()} initialPly="2" />);
    const sw = screen.getByRole("switch", { name: "Show the engine's next move" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    fireEvent.click(sw);
    expect(sw).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(window, { key: "b" });
    expect(sw).toHaveAttribute("aria-checked", "false");
  });
});
