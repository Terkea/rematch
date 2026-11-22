import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { type Annotation, AnnotationLayer } from "../src/annotations/index.js";
import { EvalBar } from "../src/board/index.js";
import {
  EvalGraph,
  type EvalGraphPoint,
  type LabelMark,
  placeLabels,
} from "../src/eval-graph/index.js";
import { ReviewControls, Scoresheet, type ScoresheetMove } from "../src/review/index.js";

// M5a Task 10: the T7 critic's fixes to the shared review pieces (critique-t7.md, changes 1-4).

let measured = 900;
class FixedResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element) {
    const width = measured;
    const rect = { width, height: 0, top: 0, left: 0, right: width, bottom: 0, x: 0, y: 0 };
    this.callback(
      [{ target, contentRect: rect } as unknown as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }
  unobserve() {}
  disconnect() {}
}
beforeAll(() => vi.stubGlobal("ResizeObserver", FixedResizeObserver));
afterEach(() => {
  cleanup();
  measured = 900;
});

const nextFrame = () =>
  act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });

async function graph(points: EvalGraphPoint[], props: Record<string, unknown> = {}) {
  let result: ReturnType<typeof render> | undefined;
  await act(async () => {
    result = render(<EvalGraph points={points} label="Win chance" {...props} />);
  });
  await nextFrame();
  return result as ReturnType<typeof render>;
}

const mk = (
  ply: number,
  san: string,
  judgement: LabelMark["judgement"],
  x: number,
  mine: boolean | null,
): LabelMark => ({ ply, san, judgement, x, top: 60, mine });

/** A long game with a judged move every few plies. */
function longGame(n: number): EvalGraphPoint[] {
  return Array.from({ length: n }, (_, i) => {
    const ply = i + 1;
    const judgement =
      ply % 23 === 0 ? "blunder" : ply % 7 === 0 ? "mistake" : ply % 5 === 0 ? "inaccuracy" : null;
    return { ply, winWhite: 50 + 40 * Math.sin(i / 11), san: "Nf3", judgement };
  });
}

describe("graph labels", () => {
  it("never leaves a bare glyph: a label that can't be placed is dropped", () => {
    const labels = placeLabels(
      [
        mk(55, "Rxe6", "blunder", 100, true),
        mk(57, "Nf3", "mistake", 230, true),
        mk(60, "exd8=Q+", "blunder", 180, false),
      ],
      320,
      { narrow: true, wide: false },
    );
    expect(labels.every((l) => l.text.san !== "")).toBe(true);
    expect(labels.map((l) => l.ply).sort()).toEqual([55, 57]);
  });

  it("labels only blunders when sparse", () => {
    const labels = placeLabels(
      [mk(10, "Nf3", "mistake", 100, true), mk(20, "Qh5", "blunder", 300, true)],
      900,
      { narrow: false, wide: true, sparse: true },
    );
    expect(labels.map((l) => l.ply)).toEqual([20]);
  });

  it("goes sparse on a long game: only ?? above the plot, the rest as ticks", async () => {
    const { container } = await graph(longGame(179), { playerColor: "white" });
    const labelled = [...container.querySelectorAll("[data-label]")].map((l) =>
      Number(l.getAttribute("data-label")),
    );
    expect(labelled.length).toBeGreaterThan(0);
    for (const ply of labelled) expect(ply % 23).toBe(0);
    expect(container.querySelectorAll('[data-mark="pen"]').length).toBeGreaterThan(labelled.length);
  });

  it("drops the middlegame name rather than run it into the endgame's", async () => {
    await graph(longGame(179), { phases: { middlegameStartPly: 30, endgameStartPly: 36 } });
    expect(screen.getByText("Endgame")).toBeInTheDocument();
    expect(screen.queryByText("Middlegame")).toBeNull();
  });

  it("keeps both phase names when they have room", async () => {
    await graph(longGame(179), { phases: { middlegameStartPly: 30, endgameStartPly: 120 } });
    expect(screen.getByText("Middlegame")).toBeInTheDocument();
    expect(screen.getByText("Endgame")).toBeInTheDocument();
  });
});

describe("graph marks and scale", () => {
  it("grades marks by severity: only a blunder in full pen", async () => {
    const { container } = await graph(longGame(60), { playerColor: "white" });
    const mark = (ply: number) => container.querySelector(`[data-mark][data-ply="${ply}"]`);
    expect(mark(23)).toHaveAttribute("data-severity", "blunder");
    expect(mark(49)).toHaveAttribute("data-severity", "mistake");
    const stroke = (ply: number) =>
      [...(mark(ply)?.querySelectorAll("line") ?? [])].at(-1)?.getAttribute("class") ?? "";
    expect(stroke(23)).toContain("stroke-pen");
    expect(stroke(23)).not.toContain("stroke-pen/");
    expect(stroke(49)).toContain("stroke-pen/");
  });

  it("draws a scale: gridlines at 25% and 75% and a 50% mark", async () => {
    const { container } = await graph(longGame(60));
    expect(container.querySelectorAll("[data-grid]")).toHaveLength(2);
    expect(container.querySelector("[data-scale]")).toHaveTextContent("50%");
  });

  it("sets the corner labels on a solid ground, so the line never runs through them", async () => {
    const { container } = await graph(longGame(60), { playerColor: "black" });
    expect(container.querySelectorAll("[data-corner-ground]")).toHaveLength(2);
  });

  // M5a T13 critic I6: the bare phone strip didn't say what it plots. It keeps no lanes or move
  // labels, but names its ends (White at the top, the player's side marked) and the 50% line.
  it("draws a compact strip: no lanes or move labels, but its ends and 50% named", async () => {
    measured = 358;
    const { container } = await graph(longGame(60), {
      compact: true,
      height: 70,
      playerColor: "black",
    });
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("height", "70");
    expect(container.querySelectorAll("[data-label]")).toHaveLength(0);
    const corners = [...container.querySelectorAll("[data-corner]")].map((c) => c.textContent);
    expect(corners).toEqual(["White", "Black (you)"]);
    expect(container.querySelector("[data-scale]")).toHaveTextContent("50%");
    expect(screen.getByRole("slider").style.top).toBe("0px");
    expect(container.querySelector('[data-mark="pen"]')).not.toBeNull();
  });
});

describe("board marks", () => {
  it("stops a toEdge arrow on the square's edge, clear of the piece", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[{ kind: "arrow", from: "e4", to: "e6", tone: "pen", toEdge: true }]}
        orientation="white"
      />,
    );
    const d = container.querySelector("mask [data-reveal]")?.getAttribute("d") ?? "";
    const tip = Number(
      d
        .trim()
        .split(/[\s,A-Z]+/)
        .filter(Boolean)
        .at(-1),
    );
    expect(tip).toBeCloseTo(2.5 + 0.46, 3);
  });

  // Fix round 1 (review-t10 I1, critic 1): the board's badge is graded like every other mark.
  it("draws a light-weight badge distinctly from a full one", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "mark", square: "e4", glyph: "??", tone: "pen" },
          { kind: "mark", square: "d4", glyph: "?", tone: "pen", weight: "light" },
          { kind: "mark", square: "c4", glyph: "?!", tone: "pencil" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    const badge = (glyph: string) =>
      [...container.querySelectorAll('[data-mark="badge"]')].find(
        (b) => b.textContent === glyph,
      ) as Element;
    const look = (glyph: string) => {
      const b = badge(glyph);
      const disc = b.querySelectorAll("circle")[1];
      return [
        b.getAttribute("data-weight"),
        disc?.getAttribute("fill"),
        disc?.getAttribute("stroke"),
        b.querySelector("text")?.getAttribute("fill"),
      ].join("|");
    };
    expect(badge("??")).toHaveAttribute("data-weight", "full");
    expect(badge("?")).toHaveAttribute("data-weight", "light");
    // Three distinct treatments: solid pen, pen outline on paper, solid pencil.
    expect(new Set([look("??"), look("?"), look("?!")]).size).toBe(3);
    expect(look("?")).toContain("var(--color-sheet)");
    expect(look("?")).toContain("var(--color-pen)");
    // Its glyph is in the pen colour: paper-coloured text on a paper disc would vanish.
    expect(badge("?").querySelector("text")).toHaveAttribute("fill", "var(--color-pen)");
  });

  it("puts an edge badge on the square's top edge, clear of the piece's middle", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[{ kind: "mark", square: "d4", glyph: "??", tone: "pen", place: "edge" }]}
        orientation="white"
        animate={false}
      />,
    );
    // d4 seen from White: col 3, row 4. Its top-right corner is (4, 4).
    const disc = container.querySelectorAll('[data-mark="badge"] circle')[1];
    expect(Number(disc?.getAttribute("cx"))).toBeCloseTo(4, 3);
    expect(Number(disc?.getAttribute("cy"))).toBeCloseTo(4, 3);
  });

  it("keeps an edge badge on the board at the top-right corner square", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[{ kind: "mark", square: "h8", glyph: "?", tone: "pen", place: "edge" }]}
        orientation="white"
        animate={false}
      />,
    );
    const disc = container.querySelectorAll('[data-mark="badge"] circle')[1];
    const r = Number(disc?.getAttribute("r"));
    expect(Number(disc?.getAttribute("cx")) + r).toBeLessThanOrEqual(8);
    expect(Number(disc?.getAttribute("cy")) - r).toBeGreaterThanOrEqual(0);
  });

  it("moves an edge badge to the top-left corner when an arrow arrives at the top right", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "arrow", from: "f6", to: "d4", tone: "good" },
          { kind: "mark", square: "d4", glyph: "??", tone: "pen", place: "edge" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    const disc = container.querySelectorAll('[data-mark="badge"] circle')[1];
    expect(Number(disc?.getAttribute("cx"))).toBeCloseTo(3, 3);
  });

  // M5a T13 critic I7: a knight's L ran its long leg straight through the bishop on e3. With the
  // occupied squares known, the L takes the leg order that crosses no piece, and when both do,
  // the arrow goes straight (between the pieces) instead of over one.
  it("routes a knight's arrow around pieces, straight when both L's cross one", () => {
    const route = (occupied: string[]) => {
      const { container, unmount } = render(
        <AnnotationLayer
          annotations={[{ kind: "arrow", from: "f3", to: "d4", tone: "pen" }]}
          orientation="white"
          animate={false}
          occupied={new Set(occupied)}
        />,
      );
      const r = container.querySelector("[data-mark='arrow']")?.getAttribute("data-route");
      unmount();
      return r;
    };
    expect(route([])).toBe("long");
    expect(route(["f3", "d4"])).toBe("long");
    expect(route(["e3"])).toBe("short");
    expect(route(["e3", "e4"])).toBe("straight");
    expect(route(["d3"])).toBe("short");
  });

  // M5a T10 re-review m1: on the a-file the top-left corner holds the rank number.
  it("never moves an a-file edge badge onto the rank number", () => {
    for (const orientation of ["white", "black"] as const) {
      const square = orientation === "white" ? "a5" : "h4";
      const from = orientation === "white" ? "c7" : "f2";
      const { container, unmount } = render(
        <AnnotationLayer
          annotations={[
            { kind: "arrow", from, to: square, tone: "good" },
            { kind: "mark", square, glyph: "??", tone: "pen", place: "edge" },
          ]}
          orientation={orientation}
          animate={false}
        />,
      );
      const disc = container.querySelectorAll('[data-mark="badge"] circle')[1];
      const cx = Number(disc?.getAttribute("cx"));
      const cy = Number(disc?.getAttribute("cy"));
      const r = Number(disc?.getAttribute("r"));
      // The square is column 0, row 3 as seen; its rank glyph box is about x 0 to 0.2, y 3 to 3.25.
      const coversRank = cx - r < 0.2 && cy - r < 3.25 && cy + r > 3;
      expect(coversRank, `${orientation} ${square}`).toBe(false);
      // And still on the square's own column.
      expect(cx).toBeLessThanOrEqual(1);
      unmount();
    }
  });

  it("describes a toEdge arrow like any arrow", () => {
    const a: Annotation = { kind: "arrow", from: "f1", to: "c4", tone: "pen", toEdge: true };
    render(<AnnotationLayer annotations={[a]} orientation="white" />);
    expect(screen.getByText("Red arrow from f1 to c4")).toBeInTheDocument();
  });
});

describe("EvalBar ends", () => {
  it("names both ends on the phone bar, White's end by the board's orientation", () => {
    const { rerender } = render(
      <EvalBar score={{ cp: 150 }} label="Evaluation" orientation="white" ends />,
    );
    const ends = () =>
      [...screen.getByRole("meter").querySelectorAll("[data-end]")].map((e) => e.textContent);
    expect(ends()).toEqual(["White", "Black"]);
    rerender(<EvalBar score={{ cp: 150 }} label="Evaluation" orientation="black" ends />);
    expect(ends()).toEqual(["Black", "White"]);
  });

  it("keeps the plain bar unchanged without ends", () => {
    render(<EvalBar score={{ cp: 150 }} label="Evaluation" />);
    expect(screen.getByRole("meter").querySelectorAll("[data-end]")).toHaveLength(0);
  });
});

describe("thumb bar", () => {
  const props = {
    ply: 4,
    lastPly: 10,
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
  };

  it("puts previous and next, play, and the mistake jumps under the thumb", () => {
    render(<ReviewControls {...props} layout="thumb" />);
    const bar = screen.getByRole("group", { name: "Move controls, thumb bar" });
    expect(
      within(bar)
        .getAllByRole("button")
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual(["Previous mistake", "Previous move", "Autoplay", "Next move", "Next mistake"]);
    // The jumps say what they jump to, so they don't read as collapse and expand.
    expect(within(bar).getAllByText("Mistake")).toHaveLength(2);
  });

  it("keeps the full layout's names and order", () => {
    render(<ReviewControls {...props} />);
    expect(screen.getByRole("group", { name: "Move controls" })).toBeInTheDocument();
  });
});

describe("scoresheet severity", () => {
  const moves: ScoresheetMove[] = [
    { ply: 1, color: "white", san: "e4", judgement: "inaccuracy", best: false, mine: true },
    { ply: 2, color: "black", san: "e5", judgement: "blunder", best: false, mine: false },
    { ply: 3, color: "white", san: "Nf3", judgement: "mistake", best: false, mine: true },
    { ply: 4, color: "black", san: "Nc6", judgement: null, best: false, mine: false },
    { ply: 5, color: "white", san: "Bc4", judgement: "blunder", best: false, mine: true },
  ];
  const glyph = (ply: number) =>
    document.getElementById(`ply-${ply}`)?.querySelector("[data-glyph]");

  it("grades the player's glyphs: ?? full pen, ? lighter pen, ?! ink", () => {
    render(<Scoresheet moves={moves} currentPly={1} onSelectPly={vi.fn()} label="Moves" />);
    expect(glyph(5)).toHaveAttribute("data-severity", "blunder");
    expect(glyph(5)).toHaveClass("text-pen-text");
    expect(glyph(3)).toHaveClass("text-pen-text/75");
    expect(glyph(1)).toHaveClass("text-ink-soft");
    expect(glyph(1)).not.toHaveClass("text-pen-text");
    // The opponent's are graphite, whatever their severity.
    expect(glyph(2)).toHaveClass("text-ink-soft");
  });
});
