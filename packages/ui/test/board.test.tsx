import "@testing-library/jest-dom/vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildPieces, PIECE_FILES } from "../scripts/build-pieces-lib.js";
import {
  Board,
  badgeCorner,
  COORDINATE,
  coordinateBoxes,
  PIECE_SCALE,
  shaftClearance,
} from "../src/board/index.js";
import { cburnett } from "../src/board/pieces.generated.js";
import { inkPoints } from "./piece-geometry.js";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
// 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7#: Black's king on e8 is in check (mate).
const MATE = "r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4";

afterEach(cleanup);

/** react-chessboard applies `squareStyles` to the layer inside each square, after its coordinates. */
const squareLayer = (container: HTMLElement, sq: string) =>
  container.querySelector<HTMLElement>(`[data-square="${sq}"] > div:last-child`);

const square = (name: string) =>
  screen.getByRole("button", { name: new RegExp(`^${name},`) }) as HTMLButtonElement;

describe("Board keyboard access", () => {
  it("Tab focuses the grid, and ArrowUp from e1 focuses e2 (white orientation)", async () => {
    const user = userEvent.setup();
    render(<Board fen={START} label="Position" interactive onMove={() => true} />);
    await user.tab();
    expect(document.activeElement).toBe(square("e1"));
    await user.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(square("e2"));
  });

  it("only one square is in the tab order (roving tabindex)", () => {
    render(<Board fen={START} label="Position" interactive onMove={() => true} />);
    const tabbable = screen.getAllByRole("button").filter((b) => b.tabIndex === 0);
    expect(tabbable).toHaveLength(1);
  });

  it("Enter on e2, ArrowUp twice and Enter plays e2-e4", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn(() => true);
    render(<Board fen={START} label="Position" interactive onMove={onMove} />);
    square("e2").focus();
    await user.keyboard("{Enter}{ArrowUp}{ArrowUp}{Enter}");
    expect(onMove).toHaveBeenCalledTimes(1);
    expect(onMove).toHaveBeenCalledWith({ from: "e2", to: "e4" });
  });

  it("Space works like Enter", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn(() => true);
    render(<Board fen={START} label="Position" interactive onMove={onMove} />);
    square("g1").focus();
    await user.keyboard(" {ArrowUp}{ArrowUp}{ArrowLeft} ");
    expect(onMove).toHaveBeenCalledWith({ from: "g1", to: "f3" });
  });

  it("an illegal keyboard move doesn't call onMove", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn(() => true);
    render(<Board fen={START} label="Position" interactive onMove={onMove} />);
    square("e2").focus();
    await user.keyboard("{Enter}{ArrowUp}{ArrowUp}{ArrowUp}{Enter}");
    expect(onMove).not.toHaveBeenCalled();
  });

  it("Escape cancels a selection", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn(() => true);
    render(<Board fen={START} label="Position" interactive onMove={onMove} />);
    square("e2").focus();
    await user.keyboard("{Enter}");
    expect(square("e2")).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("{Escape}{ArrowUp}{ArrowUp}{Enter}");
    expect(square("e2")).not.toHaveAttribute("aria-pressed");
    expect(onMove).not.toHaveBeenCalled();
  });

  it("with orientation black, ArrowUp from e8 focuses e7", async () => {
    const user = userEvent.setup();
    render(<Board fen={START} label="Position" orientation="black" />);
    square("e8").focus();
    await user.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(square("e7"));
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(square("d7"));
  });

  it("names each square with its piece", () => {
    render(<Board fen={START} label="Position" />);
    expect(square("e1")).toHaveAttribute("aria-label", "e1, white king");
    expect(square("e5")).toHaveAttribute("aria-label", "e5, empty");
    expect(square("g8")).toHaveAttribute("aria-label", "g8, black knight");
  });

  it("is not playable unless interactive", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn(() => true);
    render(<Board fen={START} label="Position" onMove={onMove} />);
    square("e2").focus();
    await user.keyboard("{Enter}{ArrowUp}{ArrowUp}{Enter}");
    expect(onMove).not.toHaveBeenCalled();
  });
});

describe("Board states", () => {
  it("announces check and rings the king in ink on a sheet halo, never pen", () => {
    const { container } = render(
      <Board fen={MATE} label="Position" check="e8" lastMove={{ from: "h5", to: "f7" }} />,
    );
    expect(square("e8")).toHaveAttribute("aria-label", "e8, black king, check");
    const e8 = squareLayer(container, "e8");
    expect(e8?.style.boxShadow).toContain("var(--color-ink)");
    expect(e8?.style.boxShadow).toContain("var(--color-sheet)");
    expect(e8?.style.boxShadow).not.toContain("pen");
    expect(container.innerHTML).not.toContain("radial-gradient");
  });

  it("tints both last-move squares", () => {
    const { container } = render(
      <Board fen={MATE} label="Position" lastMove={{ from: "h5", to: "f7" }} />,
    );
    for (const sq of ["h5", "f7"]) {
      const el = squareLayer(container, sq);
      expect(el?.style.backgroundImage).toContain("var(--color-board-lastmove)");
    }
    const e4 = squareLayer(container, "e4");
    expect(e4?.style.backgroundImage ?? "").not.toContain("lastmove");
  });

  it("renders no overlay for annotations until Task 12 lands", () => {
    const { container } = render(
      <Board fen={START} label="Position" annotations={[]} animateAnnotations />,
    );
    expect(container.querySelector("[data-annotations]")).toBeNull();
  });
});

describe("Board pieces", () => {
  it("renders only the shipped Cburnett set, never react-chessboard's default pieces", () => {
    const { container } = render(<Board fen={START} label="Position" />);
    const pieces = container.querySelectorAll("[data-piece] svg");
    expect(pieces).toHaveLength(32);
    for (const svg of pieces) expect(svg).toHaveAttribute("data-piece-set", "cburnett");
    // react-chessboard's defaultPieces are <svg version="1.1"> with literal #000000/#ffffff paint.
    expect(container.querySelectorAll('svg[version="1.1"]')).toHaveLength(0);
    expect(container.innerHTML).not.toMatch(/#000000|#ffffff/i);
  });

  it("writes out the piece scale classes in step with PIECE_SCALE", () => {
    const { container } = render(<Board fen={START} label="Position" />);
    const root = container.querySelector("[data-board]");
    expect(root?.className).toContain(`[--piece-scale:${PIECE_SCALE.phone}]`);
    expect(root?.className).toContain(`md:[--piece-scale:${PIECE_SCALE.wide}]`);
  });

  it("paints only with the piece tokens", () => {
    const json = JSON.stringify(cburnett);
    const colours = json.match(/#[0-9a-f]{3,6}\b|rgb\(/gi) ?? [];
    expect(colours).toEqual([]);
    expect(json).toContain("var(--color-piece-dark)");
    expect(json).toContain("var(--color-piece-light)");
  });

  it("matches the committed Commons SVGs (rebuild with pnpm --filter @ci/ui pieces)", () => {
    const sources = Object.fromEntries(
      PIECE_FILES.map(({ file }) => [
        file,
        readFileSync(join(import.meta.dirname, "../assets/pieces/cburnett", file), "utf8"),
      ]),
    );
    expect(buildPieces(sources)).toEqual(cburnett);
  });
});

describe("Ruled mark geometry (Task 10)", () => {
  it("ends the shaft 0.46 short of a ringed target and 0.16 short of a bare one", () => {
    expect(shaftClearance({ ringed: true })).toBe(0.46);
    expect(shaftClearance({ ringed: false })).toBe(0.16);
  });

  it("flips the badge to the top left when the arrow arrives from the right as drawn", () => {
    expect(badgeCorner({ from: "e4", to: "d2" }, "white")).toBe("top-left");
    expect(badgeCorner({ from: "c4", to: "d2" }, "white")).toBe("top-right");
    expect(badgeCorner({ from: "d4", to: "d2" }, "white")).toBe("top-right");
    // Seen from Black, e4 is left of d2.
    expect(badgeCorner({ from: "e4", to: "d2" }, "black")).toBe("top-right");
  });
});

describe("Board piece motion", () => {
  const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
  // 1. e4 d5: White to play exd5.
  const BEFORE_CAPTURE = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
  const AFTER_CAPTURE = "rnbqkbnr/ppp1pppp/8/3P4/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 2";
  const BEFORE_CASTLE = "rnbqk2r/pppp1ppp/5n2/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
  const AFTER_CASTLE = "rnbqk2r/pppp1ppp/5n2/2b1p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 b kq - 5 4";

  function stubMotion(reduce: boolean) {
    vi.stubGlobal(
      "matchMedia",
      (query: string) =>
        ({
          matches: reduce && query.includes("reduce"),
          media: query,
          addEventListener: () => {},
          removeEventListener: () => {},
        }) as unknown as MediaQueryList,
    );
  }
  const moving = (c: HTMLElement) =>
    [...c.querySelectorAll("[data-moving]")].map((el) =>
      el.closest("[data-square]")?.getAttribute("data-square"),
    );

  afterEach(() => vi.unstubAllGlobals());

  it("lifts and travels the moved piece from its origin with full motion", () => {
    stubMotion(false);
    const { container, rerender } = render(<Board fen={START} label="Position" />);
    expect(moving(container)).toEqual([]);
    rerender(<Board fen={AFTER_E4} label="Position" lastMove={{ from: "e2", to: "e4" }} />);
    expect(moving(container)).toEqual(["e4"]);
  });

  it("knocks a captured piece off the destination", () => {
    stubMotion(false);
    const { container, rerender } = render(<Board fen={BEFORE_CAPTURE} label="Position" />);
    rerender(<Board fen={AFTER_CAPTURE} label="Position" lastMove={{ from: "e4", to: "d5" }} />);
    const victim = container.querySelector('[data-square="d5"] [data-captured] svg');
    expect(victim).toHaveAttribute("data-piece-set", "cburnett");
  });

  it("moves the rook with the king when castling", () => {
    stubMotion(false);
    const { container, rerender } = render(<Board fen={BEFORE_CASTLE} label="Position" />);
    rerender(<Board fen={AFTER_CASTLE} label="Position" lastMove={{ from: "e1", to: "g1" }} />);
    expect(moving(container).sort()).toEqual(["f1", "g1"]);
  });

  it("jumps pieces under reduced motion (no travel)", () => {
    stubMotion(true);
    const { container, rerender } = render(<Board fen={START} label="Position" />);
    rerender(<Board fen={AFTER_E4} label="Position" lastMove={{ from: "e2", to: "e4" }} />);
    expect(moving(container)).toEqual([]);
  });
});

describe("Board coordinates (controller ruling, Task 11)", () => {
  it("sets coordinates at 0.2 of the square with a 2px inset: files bottom right, ranks top left", () => {
    expect(COORDINATE.size).toBe(0.2);
    expect(COORDINATE.inset).toBe(2);
    const box = coordinateBoxes(41);
    const f = 0.2 * 41;
    // In square units: the rank box hugs the top-left corner, the file box the bottom-right one.
    expect(box.rank.top).toBeCloseTo(2 / 41);
    expect(box.rank.left).toBeCloseTo(2 / 41);
    expect(box.rank.bottom).toBeCloseTo((2 + f) / 41);
    expect(box.file.bottom).toBeCloseTo(1 - 2 / 41);
    expect(box.file.right).toBeCloseTo(1 - 2 / 41);
    expect(box.file.top).toBeCloseTo(1 - (2 + f) / 41);
  });

  // Every point each piece paints (outline sampled along its curves, plus half its stroke) must
  // stay out of both glyph boxes, extended to the square's corner, so no piece covers a
  // coordinate on any square of the first rank or the a-file (or h-file and eighth rank as Black).
  const cases: [number, number][] = [
    [41, PIECE_SCALE.phone],
    [44, PIECE_SCALE.phone],
    [60, PIECE_SCALE.phone],
    [56, PIECE_SCALE.wide],
    [80, PIECE_SCALE.wide],
  ];
  for (const [square, scale] of cases) {
    it(`no piece covers a coordinate on a ${square}px square at scale ${scale}`, () => {
      const { file, rank } = coordinateBoxes(square);
      const hits: string[] = [];
      for (const [code, nodes] of Object.entries(cburnett)) {
        for (const p of inkPoints(nodes)) {
          const x = 0.5 + (p.x / 45 - 0.5) * scale;
          const y = 0.5 + (p.y / 45 - 0.5) * scale;
          const r = (p.r / 45) * scale;
          if (x + r > file.left && y + r > file.top)
            hits.push(`${code} file ${x.toFixed(3)},${y.toFixed(3)}`);
          if (x - r < rank.right && y - r < rank.bottom)
            hits.push(`${code} rank ${x.toFixed(3)},${y.toFixed(3)}`);
        }
      }
      expect(hits.slice(0, 5)).toEqual([]);
    });
  }

  it("uses on-ink coordinates on tinted night squares (last move and selection)", () => {
    const { container } = render(
      <Board fen={MATE} label="Position" lastMove={{ from: "h5", to: "f7" }} />,
    );
    const css = container.querySelector("style[data-board-coordinates]")?.textContent ?? "";
    expect(css.match(/\[data-theme="dark"\] #board-\w+-square-(\w\d)\{/g)).toHaveLength(2);
    for (const sq of ["h5", "f7"]) expect(css).toContain(`-square-${sq}{`);
    expect(css).toContain("--board-coord-light:var(--color-on-ink)");
    expect(css).toContain("--board-coord-dark:var(--color-on-ink)");
  });
});

describe("Board selected-square coordinates", () => {
  it("turns the selected square's coordinate on-ink at night too", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Board fen={START} label="Position" interactive onMove={() => true} />,
    );
    square("e2").focus();
    await user.keyboard("{Enter}");
    const css = container.querySelector("style[data-board-coordinates]")?.textContent ?? "";
    expect(css).toContain("-square-e2{");
  });
});

// ---- Fix round 1 -------------------------------------------------------------------------------

function stubFullMotion(reduce = false) {
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: reduce && query.includes("reduce"),
        media: query,
        addEventListener: () => {},
        removeEventListener: () => {},
      }) as unknown as MediaQueryList,
  );
}
const movingSquares = (c: HTMLElement) =>
  [...c.querySelectorAll("[data-moving]")].map((el) =>
    el.closest("[data-square]")?.getAttribute("data-square"),
  );
/** The travel offset a moving piece starts from, in % of a square (translateY). */
const travelY = (c: HTMLElement, sq: string) => {
  const el = c.querySelector<HTMLElement>(`[data-square="${sq}"] [data-moving]`);
  const m = el?.style.transform.match(/translateY\((-?[\d.e-]+)%\)/);
  return m ? Number(m[1]) : 0;
};
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));

describe("Board check-square coordinates", () => {
  // Black's rook on h1 checks White's king on e1, a first-rank square that carries the "e".
  const CHECK_E1 = "4k3/8/8/8/8/8/8/4K2r w - - 0 1";

  it("puts the check square's coordinate in ink on a sheet halo, over the ring", () => {
    const { container } = render(
      <Board fen={CHECK_E1} label="Position" check="e1" lastMove={{ from: "h8", to: "h1" }} />,
    );
    const css = container.querySelector("style[data-board-coordinates]")?.textContent ?? "";
    expect(css).toMatch(
      /-square-e1\{--board-coord-light:var\(--color-ink\);--board-coord-dark:var\(--color-ink\)\}/,
    );
    expect(css).toContain(
      "-square-e1>span{-webkit-text-stroke:4px var(--color-sheet);paint-order:stroke fill}",
    );
    // It comes after the tint rules, so it wins on a tinted check square.
    expect(css.lastIndexOf("-square-e1")).toBeGreaterThan(css.lastIndexOf("-square-h1"));
  });
});

describe("Board capture and focus rings near coordinates", () => {
  // White's rook on h1 can take the rook on a1, which carries both "1" and "a".
  const FEN = "4k3/8/8/8/8/8/4K3/r6R w - - 0 1";

  it("cuts the capture ring where the glyphs sit", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Board fen={FEN} label="Position" interactive onMove={() => true} />,
    );
    square("h1").focus();
    await user.keyboard("{Enter}");
    const ring = container.querySelector('[data-square-button="a1"] [data-capture-ring]');
    const mask = ring?.getAttribute("mask")?.match(/url\(#(.+)\)/)?.[1];
    expect(mask).toBeTruthy();
    const cuts = [...container.querySelectorAll(`[id="${mask}"] [data-cut]`)].map((r) =>
      r.getAttribute("data-cut"),
    );
    expect(cuts).toEqual(["rank", "file"]);
    // A square without coordinates has no cut.
    expect(container.querySelector('[data-square-button="e4"] mask')).toBeNull();
  });
});

/** A parent that accepts every move and then shows `next`, whatever the move was. */
function Host({ next }: { next: string }) {
  const [fen, setFen] = useState(START);
  return (
    <Board
      fen={fen}
      label="Position"
      interactive
      onMove={() => {
        setFen(next);
        return true;
      }}
    />
  );
}

describe("Board pending-move guard", () => {
  const UNRELATED = "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2";
  const AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";

  afterEach(() => vi.unstubAllGlobals());

  it("animates its own move when the parent plays it", async () => {
    stubFullMotion();
    const user = userEvent.setup();
    const { container } = render(<Host next={AFTER_E4} />);
    square("e2").focus();
    await user.keyboard("{Enter}{ArrowUp}{ArrowUp}{Enter}");
    expect(movingSquares(container)).toEqual(["e4"]);
  });

  it("ignores its own move when the parent jumps to an unrelated FEN", async () => {
    stubFullMotion();
    const user = userEvent.setup();
    const { container } = render(<Host next={UNRELATED} />);
    square("e2").focus();
    await user.keyboard("{Enter}{ArrowUp}{ArrowUp}{Enter}");
    expect(movingSquares(container)).toEqual([]);
  });
});

describe("Board motion edge cases", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("promotes: the pawn travels and lands as a queen", () => {
    stubFullMotion();
    const { container, rerender } = render(
      <Board fen="4k3/P7/8/8/8/8/8/4K3 w - - 0 1" label="P" />,
    );
    rerender(
      <Board fen="Q3k3/8/8/8/8/8/8/4K3 b - - 0 1" label="P" lastMove={{ from: "a7", to: "a8" }} />,
    );
    expect(movingSquares(container)).toEqual(["a8"]);
    expect(container.querySelector('[data-square="a8"] [data-piece="wQ"]')).not.toBeNull();
    expect(travelY(container, "a8")).toBe(100);
  });

  it("promotes with a capture: the rook on b8 is knocked off", () => {
    stubFullMotion();
    const { container, rerender } = render(
      <Board fen="1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1" label="P" />,
    );
    rerender(
      <Board fen="1Q2k3/8/8/8/8/8/8/4K3 b - - 0 1" label="P" lastMove={{ from: "a7", to: "b8" }} />,
    );
    expect(movingSquares(container)).toEqual(["b8"]);
    expect(container.querySelector('[data-square="b8"] [data-captured]')).not.toBeNull();
  });

  it("captures en passant: the pawn behind the target square is knocked off", () => {
    stubFullMotion();
    const { container, rerender } = render(
      <Board fen="4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2" label="P" />,
    );
    rerender(
      <Board fen="4k3/8/3P4/8/8/8/8/4K3 b - - 0 2" label="P" lastMove={{ from: "e5", to: "d6" }} />,
    );
    const victim = container.querySelector<HTMLElement>('[data-square="d6"] [data-captured]');
    expect(victim).not.toBeNull();
    // The victim sits on d5, one square below d6 as White sees it.
    expect(victim?.style.transform).toMatch(/translateY\(100%\)/);
  });

  it("castles queenside: king to c1, rook to d1", () => {
    stubFullMotion();
    const { container, rerender } = render(
      <Board fen="4k3/8/8/8/8/8/8/R3K3 w Q - 0 1" label="P" />,
    );
    rerender(
      <Board fen="4k3/8/8/8/8/8/8/2KR4 b - - 1 1" label="P" lastMove={{ from: "e1", to: "c1" }} />,
    );
    expect(movingSquares(container).sort()).toEqual(["c1", "d1"]);
  });

  it("travels in the drawn direction when seen from Black", () => {
    stubFullMotion();
    const AFTER = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    const white = render(<Board fen={START} label="W" />);
    white.rerender(<Board fen={AFTER} label="W" lastMove={{ from: "e2", to: "e4" }} />);
    expect(travelY(white.container, "e4")).toBe(200); // starts two squares below
    white.unmount();
    const black = render(<Board fen={START} label="B" orientation="black" />);
    black.rerender(
      <Board fen={AFTER} label="B" orientation="black" lastMove={{ from: "e2", to: "e4" }} />,
    );
    expect(travelY(black.container, "e4")).toBe(-200); // starts two squares above
  });

  it("re-plans a piece still in the air from where it is drawn (M7)", async () => {
    stubFullMotion();
    const E4 = "4k3/8/8/8/4P3/8/8/4K3 b - - 0 1";
    const E5 = "4k3/8/8/4P3/8/8/8/4K3 w - - 0 1";
    const { container, rerender } = render(
      <Board fen="4k3/8/8/8/8/8/4P3/4K3 w - - 0 1" label="P" />,
    );
    rerender(<Board fen={E4} label="P" lastMove={{ from: "e2", to: "e4" }} />);
    // Lift (90ms) and part of the travel: the pawn is between e2 and e4.
    await wait(130);
    const midway = travelY(container, "e4");
    expect(midway).toBeGreaterThan(0);
    expect(midway).toBeLessThan(200);
    rerender(<Board fen={E5} label="P" lastMove={{ from: "e4", to: "e5" }} />);
    // It travels on from where it was drawn (one square plus what was left), not from e4.
    const start = travelY(container, "e5");
    expect(start).toBeGreaterThan(100);
    expect(Math.abs(start - (100 + midway))).toBeLessThan(15);
  });

  it("removes the captured piece once it has been knocked off", async () => {
    stubFullMotion();
    const BEFORE = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
    const AFTER = "rnbqkbnr/ppp1pppp/8/3P4/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 2";
    const { container, rerender } = render(<Board fen={BEFORE} label="P" />);
    rerender(<Board fen={AFTER} label="P" lastMove={{ from: "e4", to: "d5" }} />);
    expect(container.querySelector('[data-square="d5"] [data-captured]')).not.toBeNull();
    await wait(900);
    expect(container.querySelector('[data-square="d5"] [data-captured]')).toBeNull();
  });
});
