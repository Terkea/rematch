import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type GameFixture, games } from "../src/fixtures/index.js";
import {
  ClockPair,
  type ClockPairProps,
  ReviewControls,
  type ReviewControlsProps,
  Scoresheet,
  type ScoresheetMove,
} from "../src/review/index.js";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** The fixture's plies as scoresheet moves (the mapper Task 10 will own). */
function movesOf(game: GameFixture): ScoresheetMove[] {
  return game.plies.map((p) => ({
    ply: p.ply,
    color: p.color,
    san: p.san,
    judgement: p.judgement,
    best: p.bestUci !== null && p.uci === p.bestUci,
    mine: p.color === game.playerColor,
  }));
}

const blunderGame = games.middlegameBlunder;
const blunderMoves = movesOf(blunderGame);

function sheet(currentPly = 31, onSelectPly = vi.fn(), moves = blunderMoves) {
  render(
    <Scoresheet
      moves={moves}
      currentPly={currentPly}
      onSelectPly={onSelectPly}
      label="Moves, DrNykterstein v Night-King96"
    />,
  );
  return onSelectPly;
}

const plyButton = (ply: number) => {
  const el = document.getElementById(`ply-${ply}`);
  if (!el) throw new Error(`no ply-${ply}`);
  return el;
};

describe("Scoresheet", () => {
  // T5 fix round 1: every move is a 44px target on a coarse pointer (DESIGN.md "Touch targets").
  // touch-targets.spec.ts hit-tests it in a browser; this pins the class without one.
  it("gives every move cell the 44px coarse-pointer height", () => {
    sheet();
    const cells = document.querySelectorAll("button[data-ply]");
    expect(cells).toHaveLength(blunderMoves.length);
    for (const cell of cells) expect(cell.className.split(/\s+/)).toContain("pointer-coarse:h-11");
  });

  it("renders one button per ply, in game order", () => {
    sheet();
    const region = screen.getByRole("region", { name: "Moves, DrNykterstein v Night-King96" });
    const ids = within(region)
      .getAllByRole("button")
      .map((b) => b.id);
    expect(ids).toEqual(blunderMoves.map((m) => `ply-${m.ply}`));
  });

  it("puts White's move left of Black's in each numbered row", () => {
    sheet();
    const row = plyButton(31).closest("li");
    expect(row).not.toBeNull();
    const cells = within(row as HTMLElement).getAllByRole("button");
    expect(cells.map((c) => c.id)).toEqual(["ply-31", "ply-32"]);
    expect(row).toHaveTextContent(/^16\./);
  });

  it("marks only the current ply with aria-current step", () => {
    sheet(31);
    expect(plyButton(31)).toHaveAttribute("aria-current", "step");
    const current = document.querySelectorAll('[aria-current="step"]');
    expect(current).toHaveLength(1);
  });

  it("marks nothing at the start position", () => {
    sheet(0);
    expect(document.querySelectorAll('[aria-current="step"]')).toHaveLength(0);
  });

  it("moves the current mark when the ply changes", () => {
    const onSelect = vi.fn();
    const { rerender } = render(
      <Scoresheet moves={blunderMoves} currentPly={31} onSelectPly={onSelect} label="Moves" />,
    );
    rerender(
      <Scoresheet moves={blunderMoves} currentPly={32} onSelectPly={onSelect} label="Moves" />,
    );
    expect(plyButton(31)).not.toHaveAttribute("aria-current");
    expect(plyButton(32)).toHaveAttribute("aria-current", "step");
  });

  it("calls onSelectPly with the clicked ply", () => {
    const onSelect = sheet(10);
    fireEvent.click(plyButton(31));
    expect(onSelect).toHaveBeenCalledWith(31);
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("uses the latest onSelectPly even when the callback changes", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(
      <Scoresheet moves={blunderMoves} currentPly={1} onSelectPly={first} label="Moves" />,
    );
    rerender(<Scoresheet moves={blunderMoves} currentPly={1} onSelectPly={second} label="Moves" />);
    fireEvent.click(plyButton(5));
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith(5);
  });

  it("draws the player's glyph in pen and the opponent's in graphite", () => {
    sheet();
    // 28. Nd2?? is DrNykterstein's (White, the player) blunder; 31… Nf4?? is the opponent's.
    const mine = plyButton(55).querySelector("[data-glyph]");
    const theirs = plyButton(62).querySelector("[data-glyph]");
    expect(mine).toHaveTextContent("??");
    expect(mine).toHaveClass("text-pen-text");
    expect(mine).not.toHaveClass("text-ink-soft");
    expect(theirs).toHaveTextContent("??");
    expect(theirs).toHaveClass("text-ink-soft");
    expect(theirs).not.toHaveClass("text-pen-text");
  });

  it("names the move, its number and its judgement for screen readers", () => {
    sheet();
    expect(plyButton(55)).toHaveAccessibleName("Move 28, White, Nd2, blunder, your move");
    expect(plyButton(62)).toHaveAccessibleName("Move 31, Black, Nf4, blunder");
    expect(plyButton(2)).toHaveAccessibleName("Move 1, Black, Nf6, best move");
    expect(plyButton(1)).toHaveAccessibleName("Move 1, White, d4");
  });

  it("tags a best move with Best", () => {
    sheet();
    // 1… Nf6 matches the engine.
    expect(plyButton(2)).toHaveTextContent("Best");
    expect(plyButton(2).querySelector("[data-best]")).toHaveClass("text-good-text");
    expect(plyButton(4)).not.toHaveTextContent("Best");
  });

  it("starts a game that opens with Black on an empty White cell", () => {
    const moves: ScoresheetMove[] = [
      { ply: 1, color: "black", san: "e5", judgement: null, best: false, mine: false },
      { ply: 2, color: "white", san: "Nf3", judgement: null, best: false, mine: true },
    ];
    sheet(1, vi.fn(), moves);
    const rows = document.querySelectorAll("li");
    expect(rows).toHaveLength(2);
    expect(
      within(rows[0] as HTMLElement)
        .getAllByRole("button")
        .map((b) => b.id),
    ).toEqual(["ply-1"]);
    expect(rows[0]).toHaveTextContent(/^1\./);
    expect(rows[1]).toHaveTextContent(/^2\./);
  });

  it("numbers rows from startMoveNumber for a game set up mid-way", () => {
    const moves: ScoresheetMove[] = [
      { ply: 1, color: "black", san: "Kg7", judgement: null, best: false, mine: false },
      { ply: 2, color: "white", san: "Rd8", judgement: null, best: false, mine: true },
    ];
    render(
      <Scoresheet
        moves={moves}
        currentPly={1}
        onSelectPly={vi.fn()}
        label="Moves"
        startMoveNumber={30}
      />,
    );
    const rows = document.querySelectorAll("li");
    expect(rows[0]).toHaveTextContent(/^30\./);
    expect(rows[1]).toHaveTextContent(/^31\./);
    expect(plyButton(1)).toHaveAccessibleName("Move 30, Black, Kg7");
  });

  it("says so when there are no moves", () => {
    sheet(0, vi.fn(), []);
    expect(screen.getByText("No moves in this game")).toBeInTheDocument();
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });

  it("scrolls the current move into view inside its own container, never the page", () => {
    const pageScroll = vi.fn();
    Element.prototype.scrollIntoView = pageScroll;
    // jsdom has no layout: place each move by its ply, 34px a row.
    vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      const ply = Number(this.id.replace("ply-", ""));
      return Number.isFinite(ply) ? Math.ceil(ply / 2) * 34 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(34);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(340);
    sheet(81);
    const scroller = document.querySelector("[data-scoresheet-scroll]") as HTMLElement;
    expect(scroller).not.toBeNull();
    // Row 41 ends at 41 × 34 + 34 = 1428; the 340px window must end there.
    expect(scroller.scrollTop).toBe(1428 - 340);
    expect(pageScroll).not.toHaveBeenCalled();
  });
});

const side = (over: Partial<ClockPairProps["top"]> = {}): ClockPairProps["top"] => ({
  name: "Night-King96",
  rating: 2848,
  seconds: 545,
  toMove: false,
  ...over,
});

describe("ClockPair", () => {
  it("shows both names, ratings and clocks, top first", () => {
    render(
      <ClockPair
        top={side()}
        bottom={side({ name: "DrNykterstein", rating: 3177, seconds: 8.43, toMove: true })}
        spent={null}
      />,
    );
    const clocks = document.querySelectorAll("time");
    expect([...clocks].map((t) => t.textContent)).toEqual(["9:05", "0:08.4"]);
    expect(clocks[0]).toHaveAttribute("datetime", "PT9M5S");
    const text = document.body.textContent ?? "";
    expect(text.indexOf("Night-King96")).toBeLessThan(text.indexOf("DrNykterstein"));
    expect(screen.getByText("2848")).toBeInTheDocument();
  });

  it("renders no clock text for a side with seconds null", () => {
    render(
      <ClockPair top={side({ seconds: null })} bottom={side({ seconds: null })} spent={null} />,
    );
    expect(document.querySelectorAll("time")).toHaveLength(0);
    expect(document.body.textContent).not.toMatch(/\d:\d\d/);
    expect(document.body.textContent).not.toMatch(/NaN|undefined/);
  });

  it("never draws a NaN clock as zero", () => {
    render(<ClockPair top={side({ seconds: Number.NaN })} bottom={side()} spent={null} />);
    expect(document.querySelectorAll("time")).toHaveLength(1);
    expect(document.body.textContent).not.toMatch(/0:00|NaN/);
  });

  it("leaves the rating out when it is null", () => {
    render(<ClockPair top={side({ rating: null })} bottom={side()} spent={null} />);
    expect(screen.getAllByText("2848")).toHaveLength(1);
    expect(document.body.textContent).not.toMatch(/null|\(\)/);
  });

  it("puts the to-move bar beside the side to move only", () => {
    render(<ClockPair top={side()} bottom={side({ toMove: true })} spent={null} />);
    const bars = document.querySelectorAll("[data-to-move-bar]");
    expect(bars).toHaveLength(1);
    expect(bars[0]).toHaveClass("bg-ink");
    expect(bars[0]?.closest("[data-side]")).toHaveAttribute("data-side", "bottom");
    expect(screen.getByText(/to move/)).toHaveClass("sr-only");
  });

  it("reads the time spent on the move", () => {
    render(<ClockPair top={side()} bottom={side()} spent={{ seconds: 12.43, by: "top" }} />);
    expect(screen.getByText("12.4 s on this move")).toBeInTheDocument();
  });

  // M5a T13: the spent line belongs to the player who made the move, not the side to move.
  it("puts the spent line under the mover's row, for either side", () => {
    for (const by of ["top", "bottom"] as const) {
      const { container, unmount } = render(
        <ClockPair
          top={side({ name: "TopPlayer", toMove: by === "bottom" })}
          bottom={side({ name: "BottomPlayer", toMove: by === "top" })}
          spent={{ seconds: 6.9, by }}
        />,
      );
      const line = screen.getByText("6.9 s on this move").closest("[data-spent-for]");
      expect(line, by).toHaveAttribute("data-spent-for", by);
      const mover = container.querySelector(`[data-side="${by}"]`) as Element;
      const other = container.querySelector(`[data-side="${by === "top" ? "bottom" : "top"}"]`);
      // Directly after the mover's row.
      expect(mover.nextElementSibling, by).toBe(line);
      expect(other?.nextElementSibling, by).not.toBe(line);
      // And named for assistive tech.
      expect(line?.textContent, by).toBe(
        `${by === "top" ? "TopPlayer" : "BottomPlayer"}: 6.9 s on this move`,
      );
      unmount();
    }
  });

  it("keeps the same height whichever side moved", () => {
    const slots = (by: "top" | "bottom") => {
      const { container, unmount } = render(
        <ClockPair top={side()} bottom={side()} spent={{ seconds: 3, by }} />,
      );
      const n = container.querySelectorAll("[data-spent-slot]").length;
      unmount();
      return n;
    };
    expect(slots("top")).toBe(2);
    expect(slots("bottom")).toBe(2);
  });

  it("shows no spent line for null or NaN", () => {
    const { rerender } = render(<ClockPair top={side()} bottom={side()} spent={null} />);
    expect(screen.queryByText(/on this move/)).toBeNull();
    rerender(
      <ClockPair top={side()} bottom={side()} spent={{ seconds: Number.NaN, by: "bottom" }} />,
    );
    expect(screen.queryByText(/on this move/)).toBeNull();
  });
});

const NAMES = [
  "First move",
  "Previous move",
  "Autoplay",
  "Next move",
  "Last move",
  "Previous mistake",
  "Next mistake",
  "Flip board",
];

function controls(over: Partial<ReviewControlsProps> = {}) {
  const props: ReviewControlsProps = {
    ply: 10,
    lastPly: 91,
    playing: false,
    hasMoments: true,
    onFirst: vi.fn(),
    onPrev: vi.fn(),
    onPlayToggle: vi.fn(),
    onNext: vi.fn(),
    onLast: vi.fn(),
    onFlip: vi.fn(),
    onPrevMoment: vi.fn(),
    onNextMoment: vi.fn(),
    ...over,
  };
  render(<ReviewControls {...props} />);
  return props;
}

const button = (name: string) => screen.getByRole("button", { name });

describe("ReviewControls", () => {
  it("labels every button, in order", () => {
    controls();
    expect(screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(NAMES);
  });

  it("disables first and previous at ply 0, and nothing else", () => {
    controls({ ply: 0 });
    expect(button("First move")).toBeDisabled();
    expect(button("Previous move")).toBeDisabled();
    expect(button("Next move")).toBeEnabled();
    expect(button("Last move")).toBeEnabled();
    expect(button("Previous mistake")).toBeEnabled();
  });

  it("disables next and last at the last ply", () => {
    controls({ ply: 91 });
    expect(button("Next move")).toBeDisabled();
    expect(button("Last move")).toBeDisabled();
    expect(button("Previous move")).toBeEnabled();
    expect(button("First move")).toBeEnabled();
  });

  it("disables the mistake buttons without moments", () => {
    controls({ hasMoments: false });
    expect(button("Previous mistake")).toBeDisabled();
    expect(button("Next mistake")).toBeDisabled();
    expect(button("Next move")).toBeEnabled();
  });

  it("calls each handler from its own button", () => {
    const p = controls();
    const pairs: [string, keyof ReviewControlsProps][] = [
      ["First move", "onFirst"],
      ["Previous move", "onPrev"],
      ["Autoplay", "onPlayToggle"],
      ["Next move", "onNext"],
      ["Last move", "onLast"],
      ["Previous mistake", "onPrevMoment"],
      ["Next mistake", "onNextMoment"],
      ["Flip board", "onFlip"],
    ];
    for (const [name, handler] of pairs) {
      fireEvent.click(button(name));
      expect(p[handler]).toHaveBeenCalledTimes(1);
    }
  });

  it("does not call a disabled handler", () => {
    const p = controls({ ply: 0 });
    fireEvent.click(button("Previous move"));
    expect(p.onPrev).not.toHaveBeenCalled();
  });

  it("keeps the name Autoplay and reports the state with aria-pressed", () => {
    controls({ playing: true });
    expect(button("Autoplay")).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
    cleanup();
    controls({ playing: false });
    expect(button("Autoplay")).toHaveAttribute("aria-pressed", "false");
  });

  it("names the key of every button", () => {
    controls();
    expect(button("Next move")).toHaveAttribute("aria-keyshortcuts", "ArrowRight");
    expect(button("Previous move")).toHaveAttribute("aria-keyshortcuts", "ArrowLeft");
    expect(button("First move")).toHaveAttribute("aria-keyshortcuts", "Home");
    expect(button("Last move")).toHaveAttribute("aria-keyshortcuts", "End");
    expect(button("Autoplay")).toHaveAttribute("aria-keyshortcuts", "Space");
    expect(button("Previous mistake")).toHaveAttribute("aria-keyshortcuts", "ArrowUp");
    expect(button("Next mistake")).toHaveAttribute("aria-keyshortcuts", "ArrowDown");
    expect(button("Flip board")).toHaveAttribute("aria-keyshortcuts", "F");
  });
});

describe("ReviewControls focus", () => {
  const props = (ply: number): ReviewControlsProps => ({
    ply,
    lastPly: 91,
    playing: false,
    hasMoments: true,
    onFirst: () => {},
    onPrev: () => {},
    onPlayToggle: () => {},
    onNext: () => {},
    onLast: () => {},
    onFlip: () => {},
    onPrevMoment: () => {},
    onNextMoment: () => {},
  });

  it("moves focus to Previous move when Next move becomes disabled at the end", () => {
    const { rerender } = render(<ReviewControls {...props(90)} />);
    button("Next move").focus();
    rerender(<ReviewControls {...props(91)} />);
    expect(button("Previous move")).toHaveFocus();
  });

  it("moves focus to Next move when Previous move becomes disabled at the start", () => {
    const { rerender } = render(<ReviewControls {...props(1)} />);
    button("Previous move").focus();
    rerender(<ReviewControls {...props(0)} />);
    expect(button("Next move")).toHaveFocus();
  });

  it("leaves focus alone while the focused button stays enabled", () => {
    const { rerender } = render(<ReviewControls {...props(40)} />);
    button("Next move").focus();
    rerender(<ReviewControls {...props(41)} />);
    expect(button("Next move")).toHaveFocus();
  });
});

describe("Scoresheet announcements", () => {
  const status = () => screen.getByRole("status");

  function view(ply: number, announce?: boolean) {
    return (
      <Scoresheet
        moves={blunderMoves}
        currentPly={ply}
        onSelectPly={() => {}}
        label="Moves"
        {...(announce === undefined ? {} : { announce })}
      />
    );
  }

  it("is a polite, atomic, visually hidden status that is empty on mount", () => {
    vi.useFakeTimers();
    try {
      render(view(31));
      act(() => vi.advanceTimersByTime(1000));
      expect(status()).toHaveAttribute("aria-live", "polite");
      expect(status()).toHaveAttribute("aria-atomic", "true");
      expect(status()).toHaveClass("sr-only");
      expect(status()).toBeEmptyDOMElement();
    } finally {
      vi.useRealTimers();
    }
  });

  it("announces the move landed on, 300 ms after the last step", () => {
    vi.useFakeTimers();
    try {
      const { rerender } = render(view(52));
      for (const ply of [53, 54, 55]) {
        rerender(view(ply));
        act(() => vi.advanceTimersByTime(100));
      }
      expect(status()).toBeEmptyDOMElement();
      act(() => vi.advanceTimersByTime(300));
      expect(status()).toHaveTextContent("Move 28, White, Nd2, blunder, your move");
    } finally {
      vi.useRealTimers();
    }
  });

  it("announces the start position", () => {
    vi.useFakeTimers();
    try {
      const { rerender } = render(view(3));
      rerender(view(0));
      act(() => vi.advanceTimersByTime(300));
      expect(status()).toHaveTextContent("Start position");
    } finally {
      vi.useRealTimers();
    }
  });

  it("stays quiet with announce false, then announces where it stopped", () => {
    vi.useFakeTimers();
    try {
      const { rerender } = render(view(10, false));
      for (const ply of [11, 12, 13]) {
        rerender(view(ply, false));
        act(() => vi.advanceTimersByTime(1000));
      }
      expect(status()).toBeEmptyDOMElement();
      rerender(view(13, true));
      act(() => vi.advanceTimersByTime(300));
      expect(status()).toHaveTextContent("Move 7, White");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("Scoresheet keyboard", () => {
  function withNeighbours(ply = 31, onSelect = vi.fn()) {
    render(
      <>
        <button type="button">Before</button>
        <Scoresheet moves={blunderMoves} currentPly={ply} onSelectPly={onSelect} label="Moves" />
        <button type="button">After</button>
      </>,
    );
    return onSelect;
  }

  const tabbable = () =>
    [...document.querySelectorAll<HTMLElement>("[id^=ply-]")].filter((b) => b.tabIndex >= 0);

  it("is one tab stop, on the current move", () => {
    withNeighbours(31);
    expect(tabbable().map((b) => b.id)).toEqual(["ply-31"]);
  });

  it("puts the tab stop on the first move at the start position", () => {
    withNeighbours(0);
    expect(tabbable().map((b) => b.id)).toEqual(["ply-1"]);
  });

  it("Tab passes the whole sheet in one press", async () => {
    const user = userEvent.setup();
    withNeighbours(31);
    screen.getByRole("button", { name: "Before" }).focus();
    await user.tab();
    expect(plyButton(31)).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
  });

  it("moves focus with the arrows, without selecting, and keeps the keys from the page", () => {
    const onSelect = withNeighbours(31);
    plyButton(31).focus();
    const right = new KeyboardEvent("keydown", {
      key: "ArrowRight",
      bubbles: true,
      cancelable: true,
    });
    plyButton(31).dispatchEvent(right);
    expect(right.defaultPrevented).toBe(true);
    expect(plyButton(32)).toHaveFocus();
    fireEvent.keyDown(plyButton(32), { key: "ArrowDown" });
    expect(plyButton(34)).toHaveFocus();
    fireEvent.keyDown(plyButton(34), { key: "ArrowLeft" });
    expect(plyButton(33)).toHaveFocus();
    fireEvent.keyDown(plyButton(33), { key: "ArrowUp" });
    expect(plyButton(31)).toHaveFocus();
    fireEvent.keyDown(plyButton(31), { key: "End" });
    expect(plyButton(91)).toHaveFocus();
    fireEvent.keyDown(plyButton(91), { key: "ArrowRight" });
    expect(plyButton(91)).toHaveFocus();
    fireEvent.keyDown(plyButton(91), { key: "Home" });
    expect(plyButton(1)).toHaveFocus();
    fireEvent.keyDown(plyButton(1), { key: "ArrowLeft" });
    expect(plyButton(1)).toHaveFocus();
    expect(onSelect).not.toHaveBeenCalled();
    expect(tabbable().map((b) => b.id)).toEqual(["ply-1"]);
  });

  it("leaves keys with a modifier to the page", () => {
    withNeighbours(31);
    plyButton(31).focus();
    fireEvent.keyDown(plyButton(31), { key: "ArrowRight", ctrlKey: true });
    expect(plyButton(31)).toHaveFocus();
  });

  it("jumps to the focused move with Enter or Space", async () => {
    const user = userEvent.setup();
    const onSelect = withNeighbours(31);
    plyButton(31).focus();
    await user.keyboard("{ArrowRight}{Enter}");
    expect(onSelect).toHaveBeenLastCalledWith(32);
    await user.keyboard("{ArrowRight} ");
    expect(onSelect).toHaveBeenLastCalledWith(33);
  });

  it("keeps focus on the current move when the ply changes under it", () => {
    const { rerender } = render(
      <Scoresheet moves={blunderMoves} currentPly={31} onSelectPly={() => {}} label="Moves" />,
    );
    plyButton(31).focus();
    rerender(
      <Scoresheet moves={blunderMoves} currentPly={32} onSelectPly={() => {}} label="Moves" />,
    );
    expect(plyButton(32)).toHaveFocus();
    expect(tabbable().map((b) => b.id)).toEqual(["ply-32"]);
  });

  it("leaves focus outside the sheet alone when the ply changes", () => {
    const tree = (ply: number) => (
      <>
        <button type="button">Elsewhere</button>
        <Scoresheet moves={blunderMoves} currentPly={ply} onSelectPly={() => {}} label="Moves" />
      </>
    );
    const { rerender } = render(tree(31));
    screen.getByRole("button", { name: "Elsewhere" }).focus();
    rerender(tree(32));
    expect(screen.getByRole("button", { name: "Elsewhere" })).toHaveFocus();
    expect(tabbable().map((b) => b.id)).toEqual(["ply-32"]);
  });
});

describe("Scoresheet scrolling", () => {
  function layout(clientHeight = 340) {
    Element.prototype.scrollIntoView = vi.fn();
    vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      const ply = Number(this.id.replace("ply-", ""));
      return this.id.startsWith("ply-") ? Math.ceil(ply / 2) * 34 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(34);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(clientHeight);
  }
  const scroller = () => document.querySelector("[data-scoresheet-scroll]") as HTMLElement;
  const view = (ply: number, moves = blunderMoves) => (
    <Scoresheet moves={moves} currentPly={ply} onSelectPly={() => {}} label="Moves" />
  );

  it("scrolls back up to a move above the window", () => {
    layout();
    const { rerender } = render(view(81));
    scroller().scrollTop = 1000;
    rerender(view(5));
    // Ply 5 is row 3, whose top is 3 × 34.
    expect(scroller().scrollTop).toBe(102);
  });

  it("leaves the scroll alone when the move is already in view", () => {
    layout();
    const { rerender } = render(view(81));
    scroller().scrollTop = 1000;
    rerender(view(65));
    expect(scroller().scrollTop).toBe(1000);
  });

  it("scrolls when the moves arrive after the ply", () => {
    layout();
    const { rerender } = render(view(81, []));
    rerender(view(81));
    expect(scroller().scrollTop).toBe(1428 - 340);
  });
});

describe("ClockPair details", () => {
  it("marks a missing clock with a quiet dash when the other side has one", () => {
    render(<ClockPair top={side({ seconds: null })} bottom={side()} spent={null} />);
    const missing = document.querySelector('[data-side="top"] [data-no-clock]');
    expect(missing).not.toBeNull();
    expect(missing?.querySelector(".sr-only")).toHaveTextContent("no clock");
    expect(document.querySelectorAll("time")).toHaveLength(1);
  });

  it("shows neither clock nor dash when no side has a clock", () => {
    render(
      <ClockPair top={side({ seconds: null })} bottom={side({ seconds: null })} spent={null} />,
    );
    expect(document.querySelectorAll("[data-no-clock]")).toHaveLength(0);
  });

  it("shows whole seconds when the source has no tenths", () => {
    render(
      <ClockPair
        top={side({ seconds: 8 })}
        bottom={side()}
        spent={{ seconds: 12, by: "top" }}
        tenths={false}
      />,
    );
    expect(document.querySelectorAll("time")[0]).toHaveTextContent(/^0:08$/);
    expect(screen.getByText("12 s on this move")).toBeInTheDocument();
  });
});

describe("ReviewControls focus, blurred", () => {
  it("does not pull focus back from the page once the controls lost it", () => {
    const p = (ply: number): ReviewControlsProps => ({
      ply,
      lastPly: 91,
      playing: false,
      hasMoments: true,
      onFirst: () => {},
      onPrev: () => {},
      onPlayToggle: () => {},
      onNext: () => {},
      onLast: () => {},
      onFlip: () => {},
      onPrevMoment: () => {},
      onNextMoment: () => {},
    });
    const { rerender } = render(<ReviewControls {...p(90)} />);
    button("Next move").focus();
    button("Next move").blur();
    rerender(<ReviewControls {...p(91)} />);
    expect(document.body).toHaveFocus();
  });
});
