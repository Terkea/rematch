import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  EvalGraph,
  type EvalGraphPoint,
  type EvalGraphProps,
  type LabelMark,
  type PlacedLabel,
  placeLabels,
  xForPly,
  yForWin,
} from "../src/eval-graph/index.js";
import {
  collisions,
  cornerBox,
  cornerWidth,
  GRAPH_NARROW,
  LABEL_WIDE,
  leaderPoints,
  PAD,
  type Point,
  placeCorners,
  READOUT_W,
  readoutPlacement,
  segmentHitsBox,
} from "../src/eval-graph/scales.js";
import { type GameFixture, games } from "../src/fixtures/index.js";
import { tokensCss } from "./design-md.js";

const WIDTH = 320;
/** The width the ResizeObserver stub reports; tests at real board widths change it. */
let measured = WIDTH;
// At 320px the container is narrow: an 88px plot under a 22px label lane.
const LANE = 22;
const PLOT_H = 88;

// jsdom has no layout: report every observed element as WIDTH wide.
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

beforeAll(() => {
  vi.stubGlobal("ResizeObserver", FixedResizeObserver);
});

afterEach(() => {
  cleanup();
  measured = WIDTH;
});

const fixturePoints = (): EvalGraphPoint[] =>
  games.middlegameBlunder.plies.map((p) => ({
    ply: p.ply,
    winWhite: p.winWhite,
    san: p.san,
    judgement: p.judgement,
  }));

const toPoints = (game: GameFixture): EvalGraphPoint[] =>
  game.plies.map((p) => ({ ply: p.ply, winWhite: p.winWhite, san: p.san, judgement: p.judgement }));

/** A fixture's marks as the graph builds them at `width`. */
function fixtureMarks(game: GameFixture, width: number): LabelMark[] {
  const plotH = width < GRAPH_NARROW ? 88 : 120;
  const n = game.plies.length;
  return game.plies.flatMap((p, i) => {
    if (!p.judgement) return [];
    const y = LANE + yForWin(p.winWhite, plotH);
    const half = p.judgement === "blunder" ? 9 : p.judgement === "mistake" ? 7 : 5;
    return [
      {
        ply: p.ply,
        san: p.san,
        judgement: p.judgement,
        x: PAD + xForPly(i + 1, width - 2 * PAD, n),
        top: Math.max(LANE + 2, y - half),
        mine: p.color === game.playerColor,
      },
    ];
  });
}

const pairs = (s: string | null): Point[] =>
  (s ?? "")
    .trim()
    .split(/\s+/)
    .map((xy) => xy.split(",").map(Number) as unknown as Point);

const orient = (a: Point, b: Point, c: Point) =>
  Math.sign((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]));

const onSegment = (a: Point, b: Point, c: Point) =>
  Math.min(a[0], b[0]) <= c[0] &&
  c[0] <= Math.max(a[0], b[0]) &&
  Math.min(a[1], b[1]) <= c[1] &&
  c[1] <= Math.max(a[1], b[1]);

function segmentsCross(p1: Point, p2: Point, q1: Point, q2: Point): boolean {
  const o1 = orient(p1, p2, q1);
  const o2 = orient(p1, p2, q2);
  const o3 = orient(q1, q2, p1);
  const o4 = orient(q1, q2, p2);
  if (o1 !== o2 && o3 !== o4) return true;
  return (
    (o1 === 0 && onSegment(p1, p2, q1)) ||
    (o2 === 0 && onSegment(p1, p2, q2)) ||
    (o3 === 0 && onSegment(q1, q2, p1)) ||
    (o4 === 0 && onSegment(q1, q2, p2))
  );
}

/** Whether two leader polylines touch anywhere. */
function crosses(a: readonly Point[], b: readonly Point[]): boolean {
  for (let i = 1; i < a.length; i++) {
    for (let j = 1; j < b.length; j++) {
      if (segmentsCross(a[i - 1] as Point, a[i] as Point, b[j - 1] as Point, b[j] as Point)) {
        return true;
      }
    }
  }
  return false;
}

const nextFrame = () =>
  act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });

async function renderGraph(props: Partial<EvalGraphProps> = {}) {
  let result: ReturnType<typeof render> | undefined;
  await act(async () => {
    result = render(<EvalGraph points={fixturePoints()} label="Win chance" {...props} />);
  });
  // useParentSize applies the measured width on the next animation frame.
  await nextFrame();
  return result as ReturnType<typeof render>;
}

const numbersOf = (s: string | null) => (s ?? "").split(/[ ,]/).filter(Boolean).map(Number);

describe("scales", () => {
  it("maps win percent to y with White at the top", () => {
    expect(yForWin(50, 160)).toBe(80);
    expect(yForWin(100, 160)).toBe(0);
    expect(yForWin(0, 160)).toBe(160);
    expect(yForWin(120, 160)).toBe(0);
  });

  it("spreads plies across the width", () => {
    expect(xForPly(1, 320, 91)).toBe(0);
    expect(xForPly(91, 320, 91)).toBe(320);
    expect(xForPly(1, 320, 1)).toBe(160);
  });
});

const mk = (
  ply: number,
  san: string,
  judgement: LabelMark["judgement"],
  x: number,
  mine: boolean | null,
  top = 60,
): LabelMark => ({ ply, san, judgement, x, top, mine });

describe("placeLabels", () => {
  const narrow = { narrow: true, wide: false };

  it("uses SAN plus glyph below 512px and full notation when there is room", () => {
    const [small] = placeLabels([mk(55, "Nd2", "blunder", 100, true)], 320, narrow);
    expect(small?.text).toEqual({ prefix: "", san: "Nd2", glyph: "??" });
    const [big] = placeLabels([mk(55, "Nd2", "blunder", 100, true)], 900, {
      narrow: false,
      wide: true,
    });
    expect(big?.text).toEqual({ prefix: "28. ", san: "Nd2", glyph: "??" });
  });

  it("places the player's blunders before the opponent's", () => {
    const labels = placeLabels(
      [mk(62, "Nf4", "blunder", 100, false), mk(55, "Nd2", "blunder", 100, true)],
      320,
      narrow,
    );
    expect(labels.find((l) => l.ply === 55)?.x).toBe(94);
    expect(labels.find((l) => l.ply === 62)?.x).not.toBe(94);
  });

  // M5a T10: the glyph-only fallback is gone (a bare "?" read as noise); see
  // review-page-fixes.test.tsx, "never leaves a bare glyph".
  it("drops a label that can't be placed", () => {
    const labels = placeLabels(
      [
        mk(55, "Rxe6", "blunder", 100, true),
        mk(57, "Nf3", "mistake", 230, true),
        mk(60, "exd8=Q+", "blunder", 150, false),
      ],
      320,
      narrow,
    );
    expect(labels.map((l) => l.ply).sort()).toEqual([55, 57]);
  });

  it("uses one label form for the whole lane", () => {
    const labels = placeLabels(
      [
        mk(55, "Nd2", "blunder", 100, true),
        mk(57, "Nd2", "blunder", 150, true),
        mk(59, "Nd2", "blunder", 200, true),
      ],
      600,
      { narrow: false, wide: false },
    );
    expect(labels).toHaveLength(3);
    for (const l of labels) expect(l.text.prefix).toBe("");
  });

  it("labels inaccuracies only on wide lanes, clear of serious labels and spaced", () => {
    const marks = [
      mk(39, "Qd3", "mistake", 100, null),
      mk(40, "fxe4", "inaccuracy", 170, null),
      mk(46, "Bxh3+", "inaccuracy", 300, null),
      mk(50, "Nxb4", "inaccuracy", 380, null),
      mk(56, "e4", "inaccuracy", 500, null),
    ];
    const wide = placeLabels(marks, 900, { narrow: false, wide: true });
    expect(wide.filter((l) => !l.serious).map((l) => l.ply)).toEqual([46, 56]);
    expect(placeLabels(marks, 600, { narrow: false, wide: false })).toHaveLength(1);
  });
});

describe("leaders and corners", () => {
  const widths = [320, 326, 448, 496, 512, 560, 1176];
  const sets: [string, GameFixture][] = [
    ["middlegameBlunder", games.middlegameBlunder],
    ["endgameGrind", games.endgameGrind],
    ["shortAttack", games.shortAttack],
  ];

  it.each(sets)("never crosses two leaders in %s", (_, game) => {
    for (const width of widths) {
      const labels = placeLabels(fixtureMarks(game, width), width, {
        narrow: width < GRAPH_NARROW,
        wide: width >= LABEL_WIDE,
      });
      const led = labels.filter((l) => l.leader);
      for (const a of led) {
        for (const b of led) {
          if (a.ply >= b.ply) continue;
          expect(a.x < b.x, `labels ${a.ply}/${b.ply} at ${width}px`).toBe(a.tickX < b.tickX);
          expect(crosses(leaderPoints(a), leaderPoints(b)), `${a.ply}/${b.ply} at ${width}px`).toBe(
            false,
          );
        }
      }
    }
  });

  it("keeps game 2's Nb4?? and f4? in tick order", () => {
    const labels = placeLabels(fixtureMarks(games.endgameGrind, 1176), 1176, {
      narrow: false,
      wide: true,
    });
    const nb4 = labels.find((l) => l.ply === 25);
    const f4 = labels.find((l) => l.ply === 26);
    expect(nb4?.leader).toBe(true); // the player's blunder keeps its label
    // f4? may be dropped (it can't sit left of Nb4??'s label), but never placed out of order.
    if (f4) expect(f4.x).toBeGreaterThan(nb4?.x ?? 0);
    if (f4?.leader) expect(crosses(leaderPoints(nb4 as PlacedLabel), leaderPoints(f4))).toBe(false);
  });

  it("gives a tick at the top of the plot a straight vertical leader", () => {
    const [label] = placeLabels([mk(55, "Nd2", "blunder", 100, true, LANE + 2)], 320, {
      narrow: true,
      wide: false,
    });
    expect(label).toBeDefined();
    const pts = leaderPoints(label as PlacedLabel);
    expect(pts).toHaveLength(2);
    expect(pts.every(([x]) => x === 100)).toBe(true);
  });

  it("flips a corner label only when the right end is completely clear", () => {
    const base = {
      width: 326,
      plotH: PLOT_H,
      top: "White (you)",
      bottom: "Black",
      ticks: [],
      line: [] as Point[],
    };
    expect(placeCorners({ ...base, labels: [] })).toEqual({ top: "left", bottom: "left" });
    const through: PlacedLabel = {
      ply: 25,
      x: 10,
      width: 40,
      tickX: 30,
      tickTop: 80,
      serious: true,
      leader: true,
      mine: true,
      text: { prefix: "", san: "Nb4", glyph: "??" },
    };
    expect(placeCorners({ ...base, labels: [through] })).toEqual({ top: "right", bottom: "left" });
    // A line skimming the right end's cap height (within the 3px clearance) keeps it left.
    const skim: Point[] = [
      [200, LANE + 3],
      [320, LANE + 3],
    ];
    expect(placeCorners({ ...base, labels: [through], line: skim }).top).toBe("left");
    // So does a tick there.
    const tick = { x: 300, y1: LANE + 2, y2: LANE + 20 };
    expect(placeCorners({ ...base, labels: [through], ticks: [tick] }).top).toBe("left");
  });
});

describe("placeCorners on the phone strip (no label lane)", () => {
  it("measures the corner boxes from the strip's own top, not the full graph's lane", () => {
    // The strip draws its names at y 14 (top) and plotH - 6 (bottom), with no lane above the
    // plot (M5a parked polish: "White" sat over the line's last moves at the top right).
    const line: Point[] = [
      [0, 30],
      [200, 30],
      [326, 8],
    ];
    const sides = placeCorners({
      width: 326,
      plotH: 70,
      lane: 0,
      top: "White",
      bottom: "Black (you)",
      labels: [],
      ticks: [],
      line,
    });
    expect(sides.top).toBe("left");
    expect(cornerBox("top", "left", 34, 326, 70, 0).y0).toBe(14 - 9 - 3);
  });
});

describe("readoutPlacement", () => {
  const flat = { xs: [] as number[], ys: [] as number[], plotH: 120, width: 560, lineY: 100 };
  const corners = (side: "left" | "right") => ({
    top: { side, width: 70 },
    bottom: { side, width: 40 },
  });

  it("keeps clear of a corner label on the right", () => {
    const { left, top } = readoutPlacement({ ...flat, x: 540, corners: corners("right") });
    const reserved = PAD + (top === LANE + 6 ? 70 : 40) + 8;
    expect(left + READOUT_W).toBeLessThanOrEqual(560 - reserved);
  });

  it("keeps clear of a corner label on the left", () => {
    const { left, top } = readoutPlacement({ ...flat, x: 10, corners: corners("left") });
    expect(left).toBeGreaterThanOrEqual(PAD + (top === LANE + 6 ? 70 : 40) + 8);
  });

  it("opens on the side away from the cursor rule, never over it", () => {
    const toLeft = readoutPlacement({ ...flat, x: 300, cursorX: 340, corners: corners("left") });
    expect(toLeft.left + READOUT_W).toBeLessThan(300);
    const toRight = readoutPlacement({ ...flat, x: 300, cursorX: 260, corners: corners("left") });
    expect(toRight.left).toBeGreaterThan(300);
    for (const cursorX of [120, 200, 260, 340, 420]) {
      const { left } = readoutPlacement({ ...flat, x: 300, cursorX, corners: corners("left") });
      expect(cursorX < left || cursorX > left + READOUT_W).toBe(true);
    }
  });
});

describe("tokens", () => {
  it("shares the graph threshold and heights with tokens.css", () => {
    const css = tokensCss;
    expect(css).toContain(`--container-graph: ${GRAPH_NARROW}px;`);
    expect(css).toContain(`--eval-graph-height: ${LANE + 88 + 18}px;`);
    expect(css).toContain(`--eval-graph-height-wide: ${LANE + 120 + 18}px;`);
  });
});

describe("EvalGraph", () => {
  it("draws one pen tick per mistake or blunder and a pencil tick per inaccuracy", async () => {
    const { container } = await renderGraph();
    const count = (j: string[]) =>
      games.middlegameBlunder.plies.filter((p) => p.judgement && j.includes(p.judgement)).length;
    expect(count(["mistake", "blunder"])).toBe(7);
    expect(container.querySelectorAll('[data-mark="pen"]')).toHaveLength(7);
    expect(container.querySelectorAll('[data-mark="pencil"]')).toHaveLength(count(["inaccuracy"]));
  });

  it("keeps ticks and leaders inside the plot, with labels drawn over the marks", async () => {
    const points: EvalGraphPoint[] = Array.from({ length: 12 }, (_, i) => ({
      ply: i + 1,
      winWhite: i % 2 === 0 ? 100 : 0,
      san: "Qh5",
      judgement: "blunder",
    }));
    const { container } = await renderGraph({ points });
    for (const line of container.querySelectorAll("[data-mark] line")) {
      expect(Number(line.getAttribute("y1"))).toBeGreaterThanOrEqual(LANE + 2);
      expect(Number(line.getAttribute("y2"))).toBeLessThanOrEqual(LANE + PLOT_H - 2);
    }
    const leaders = container.querySelectorAll("polyline[data-leader]");
    expect(leaders.length).toBeGreaterThan(0);
    for (const leader of leaders) {
      const ys = numbersOf(leader.getAttribute("points")).filter((_, i) => i % 2 === 1);
      for (const y of ys) {
        expect(y).toBeGreaterThanOrEqual(18);
        expect(y).toBeLessThanOrEqual(LANE + PLOT_H);
      }
      expect(Math.max(...ys)).toBeGreaterThanOrEqual(LANE);
    }
    const marks = container.querySelector('[data-layer="marks"]');
    const labels = container.querySelector('[data-layer="labels"]');
    expect(marks && labels).toBeTruthy();
    expect(
      (marks as Element).compareDocumentPosition(labels as Element) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("steps plies from the keyboard", async () => {
    const onSelectPly = vi.fn();
    await renderGraph({ currentPly: 30, onSelectPly });
    const slider = screen.getByRole("slider", { name: "Win chance" });
    expect(slider).toHaveAttribute("aria-valuemin", "1");
    expect(slider).toHaveAttribute("aria-valuemax", "91");
    expect(slider).toHaveAttribute("aria-valuenow", "30");
    const press = (key: string) => fireEvent.keyDown(slider, { key });
    press("ArrowRight");
    expect(onSelectPly).toHaveBeenLastCalledWith(31);
    press("ArrowLeft");
    expect(onSelectPly).toHaveBeenLastCalledWith(29);
    press("PageUp");
    expect(onSelectPly).toHaveBeenLastCalledWith(40);
    press("PageDown");
    expect(onSelectPly).toHaveBeenLastCalledWith(20);
    press("End");
    expect(onSelectPly).toHaveBeenLastCalledWith(91);
    press("Home");
    expect(onSelectPly).toHaveBeenLastCalledWith(1);
  });

  it("clamps keyboard steps to the game", async () => {
    const onSelectPly = vi.fn();
    const { rerender } = await renderGraph({ currentPly: 88, onSelectPly });
    const slider = screen.getByRole("slider");
    fireEvent.keyDown(slider, { key: "PageUp" });
    expect(onSelectPly).toHaveBeenLastCalledWith(91);
    onSelectPly.mockClear();
    rerender(
      <EvalGraph
        points={fixturePoints()}
        label="Win chance"
        currentPly={91}
        onSelectPly={onSelectPly}
      />,
    );
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    rerender(
      <EvalGraph
        points={fixturePoints()}
        label="Win chance"
        currentPly={1}
        onSelectPly={onSelectPly}
      />,
    );
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    fireEvent.keyDown(slider, { key: "PageDown" });
    expect(onSelectPly).not.toHaveBeenCalled();
  });

  it("keeps its own ply when uncontrolled", async () => {
    const onSelectPly = vi.fn();
    const { container } = await renderGraph({ onSelectPly });
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuenow", "91");
    expect(container.querySelector("[data-cursor-rule]")).toBeNull();
    await act(async () => {
      fireEvent.keyDown(slider, { key: "ArrowLeft" });
    });
    expect(onSelectPly).toHaveBeenLastCalledWith(90);
    expect(slider).toHaveAttribute("aria-valuenow", "90");
    expect(slider).toHaveAttribute("aria-valuetext", "Move 45, Black Bf5, White 98%");
    expect(container.querySelector("[data-cursor-rule]")).not.toBeNull();
  });

  it("describes the current ply in words", async () => {
    const { rerender } = await renderGraph({ currentPly: 39 });
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuenow", "39");
    expect(slider).toHaveAttribute("aria-valuetext", "Move 20, White Qd3, mistake, White 72%");
    rerender(<EvalGraph points={fixturePoints()} label="Win chance" currentPly={62} />);
    expect(slider).toHaveAttribute("aria-valuetext", "Move 31, Black Nf4, blunder, White 81%");
    rerender(<EvalGraph points={fixturePoints()} label="Win chance" currentPly={1} />);
    expect(slider).toHaveAttribute("aria-valuetext", "Move 1, White d4, White 51%");
  });

  it("treats ply 0 as the start position, with no cursor and the minimum still 1", async () => {
    const onSelectPly = vi.fn();
    const { container } = await renderGraph({ currentPly: 0, onSelectPly });
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuemin", "1");
    expect(slider).toHaveAttribute("aria-valuenow", "1");
    expect(slider).toHaveAttribute("aria-valuetext", "Start position, before move 1");
    expect(container.querySelector("[data-cursor-rule]")).toBeNull();
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    expect(onSelectPly).not.toHaveBeenCalled();
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(onSelectPly).toHaveBeenLastCalledWith(1);
  });

  it("keeps the cursor rule spanning the plot at every ply", async () => {
    const { container, rerender } = await renderGraph({ currentPly: 30 });
    for (const ply of [30, 55, 62, 91]) {
      await act(async () => {
        rerender(<EvalGraph points={fixturePoints()} label="Win chance" currentPly={ply} />);
      });
      const rule = container.querySelector("[data-cursor-rule]");
      expect(rule).toHaveAttribute("y1", String(LANE));
      expect(rule).toHaveAttribute("y2", String(LANE + PLOT_H));
      expect(rule?.getAttribute("style") ?? "").not.toMatch(/translateY|translate3d/);
      expect(container.querySelector("[data-cursor-dot]")).not.toBeNull();
    }
  });

  it("draws phase boundaries, never at the first ply", async () => {
    const { container } = await renderGraph({
      phases: { middlegameStartPly: 1, endgameStartPly: 60 },
    });
    const lines = container.querySelectorAll("[data-phase]");
    expect(lines).toHaveLength(1);
    expect(lines[0]).toHaveAttribute("data-phase", "Endgame");
    expect(screen.getByText("Endgame")).toBeInTheDocument();
    expect(screen.queryByText("Middlegame")).toBeNull();
  });

  it("marks the player's side", async () => {
    await renderGraph({ playerColor: "black" });
    expect(screen.getByText("Black (you)")).toBeInTheDocument();
    expect(screen.getByText("White")).toBeInTheDocument();
  });

  it("lists only mistakes and blunders in its description", async () => {
    await renderGraph();
    const id = screen.getByRole("slider").getAttribute("aria-describedby") ?? "";
    const text = document.getElementById(id)?.textContent ?? "";
    expect(text).toContain("Move 20, White Qd3, mistake, White 72%");
    expect(text).toContain("Move 31, Black Nf4, blunder, White 81%");
    expect(text).not.toContain("fxe4");
    expect(text).toContain("Also 12 inaccuracies.");
  });

  it("reveals with a clip on the line and area only, but not under reduced motion", async () => {
    const { container, unmount } = await renderGraph({ drawIn: true, currentPly: 30 });
    expect(container.querySelector("clipPath")).not.toBeNull();
    expect(container.querySelector('[data-series="line"]')?.closest("[clip-path]")).not.toBeNull();
    for (const layer of ["marks", "labels", "leaders"]) {
      const g = container.querySelector(`[data-layer="${layer}"]`);
      expect(g).not.toBeNull();
      expect(g?.closest("[clip-path]")).toBeNull();
    }
    // Marks, labels and the cursor fade on the reveal's one clock (--wipe), not on timers.
    const style = (el: Element | null) => el?.getAttribute("style") ?? "";
    expect(style(container.querySelector('[data-mark="pen"]'))).toMatch(/var\(--wipe/);
    expect(style(container.querySelector("[data-label]"))).toMatch(/var\(--wipe/);
    expect(style(container.querySelector("[data-cursor]"))).toMatch(/var\(--wipe/);
    unmount();
    const media = { matches: true, addEventListener() {}, removeEventListener() {} };
    Object.defineProperty(window, "matchMedia", { configurable: true, value: () => media });
    try {
      const reduced = await renderGraph({ drawIn: true });
      expect(reduced.container.querySelector("clipPath")).toBeNull();
      expect(reduced.container.querySelectorAll('[data-mark="pen"]')).toHaveLength(7);
    } finally {
      Reflect.deleteProperty(window, "matchMedia");
    }
  });

  it("selects on touch only after a tap or a horizontal drag", async () => {
    const onSelectPly = vi.fn();
    await renderGraph({ currentPly: 30, onSelectPly });
    const slider = screen.getByRole("slider");
    const touch = { pointerType: "touch", pointerId: 7, button: 0, isPrimary: true };

    fireEvent.pointerDown(slider, { ...touch, clientX: 100 });
    expect(onSelectPly).not.toHaveBeenCalled();
    fireEvent.pointerCancel(slider, touch);
    expect(onSelectPly).not.toHaveBeenCalled();

    fireEvent.pointerDown(slider, { ...touch, clientX: 100 });
    fireEvent.pointerMove(slider, { ...touch, clientX: 103 });
    expect(onSelectPly).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider, { ...touch, clientX: 103 });
    expect(onSelectPly).toHaveBeenCalledTimes(1);

    onSelectPly.mockClear();
    fireEvent.pointerDown(slider, { ...touch, clientX: 100 });
    fireEvent.pointerMove(slider, { ...touch, clientX: 160 });
    expect(onSelectPly).toHaveBeenCalledTimes(1);
    fireEvent.pointerUp(slider, { ...touch, clientX: 160 });

    // A tap that wandered more than 10px in total is not a tap.
    onSelectPly.mockClear();
    fireEvent.pointerDown(slider, { ...touch, clientX: 100, clientY: 50 });
    fireEvent.pointerMove(slider, { ...touch, clientX: 104, clientY: 70 });
    fireEvent.pointerUp(slider, { ...touch, clientX: 100, clientY: 52 });
    expect(onSelectPly).not.toHaveBeenCalled();

    // A second finger is ignored.
    fireEvent.pointerDown(slider, { ...touch, pointerId: 8, isPrimary: false, clientX: 100 });
    fireEvent.pointerUp(slider, { ...touch, pointerId: 8, isPrimary: false, clientX: 100 });
    expect(onSelectPly).not.toHaveBeenCalled();
  });

  it("selects on mouse press, but not on other buttons", async () => {
    const onSelectPly = vi.fn();
    await renderGraph({ currentPly: 30, onSelectPly });
    const slider = screen.getByRole("slider");
    fireEvent.pointerDown(slider, {
      pointerType: "mouse",
      pointerId: 1,
      isPrimary: true,
      button: 2,
      clientX: 200,
    });
    expect(onSelectPly).not.toHaveBeenCalled();
    fireEvent.pointerDown(slider, {
      pointerType: "mouse",
      pointerId: 1,
      isPrimary: true,
      button: 0,
      clientX: 200,
    });
    expect(onSelectPly).toHaveBeenCalledTimes(1);
  });

  it("shows the readout only on hover, drag or keyboard focus", async () => {
    const { container } = await renderGraph({ currentPly: 30 });
    const slider = screen.getByRole("slider");
    const lane = () => container.querySelector("[data-readout-lane]");
    expect(lane()).toBeNull();
    await act(async () => {
      slider.focus();
      fireEvent.keyDown(slider, { key: "ArrowRight" });
    });
    expect(lane()).not.toBeNull();
    await act(async () => {
      slider.blur();
      fireEvent.pointerMove(slider, {
        pointerType: "mouse",
        pointerId: 1,
        isPrimary: true,
        clientX: 200,
      });
    });
    // Below 496px the readout takes the phase lane, which hides the phase labels meanwhile, and
    // sits 6px under the plot, clear of the focus ring (2px offset plus 2px width).
    expect(lane()?.textContent).toMatch(/White \d+%/);
    expect(lane()).toHaveStyle({ top: `${LANE + PLOT_H + 6}px` });
    expect(container.querySelector("[data-readout]")).not.toBeNull();
    await act(async () => {
      fireEvent.pointerLeave(slider, { pointerType: "mouse", pointerId: 1 });
    });
    expect(lane()).toBeNull();
  });

  it("shows a popover from 496px, clear of the corner labels", async () => {
    measured = 560;
    const { container } = await renderGraph({ currentPly: 30, playerColor: "white" });
    const slider = screen.getByRole("slider");
    const popover = () => container.querySelector("[data-readout-popover]") as HTMLElement | null;
    for (const clientX of [0, 280, 560]) {
      await act(async () => {
        fireEvent.pointerMove(slider, {
          pointerType: "mouse",
          pointerId: 1,
          isPrimary: true,
          clientX,
        });
      });
      expect(container.querySelector("[data-readout-lane]")).toBeNull();
      const box = popover();
      expect(box).not.toBeNull();
      const left = Number.parseFloat(box?.style.left ?? "");
      const top = Number.parseFloat(box?.style.top ?? "");
      expect(left).toBeGreaterThanOrEqual(0);
      expect(left + 148).toBeLessThanOrEqual(560);
      const band = top === LANE + 6 ? "top" : "bottom";
      const corner = container.querySelector(`[data-corner="${band}"]`);
      const reserved = PAD + cornerWidth(corner?.textContent ?? "") + 8;
      if (corner?.getAttribute("data-side") === "left") {
        expect(left).toBeGreaterThanOrEqual(reserved);
      } else {
        expect(left + 148).toBeLessThanOrEqual(560 - reserved);
      }
    }
  });

  it("moves a corner label right only onto clear ground, at 390", async () => {
    measured = 326; // the graph's width in a 390px viewport
    const game = games.endgameGrind;
    const { container } = await renderGraph({ points: toPoints(game), playerColor: "white" });
    const n = game.plies.length;
    const obstacles = {
      labels: [] as PlacedLabel[],
      line: game.plies.map(
        (p, i) =>
          [PAD + xForPly(i + 1, 326 - 2 * PAD, n), LANE + yForWin(p.winWhite, PLOT_H)] as Point,
      ),
      ticks: [...container.querySelectorAll("[data-mark] line, line[data-mark]")].map((l) => ({
        x: Number(l.getAttribute("x1")),
        y1: Number(l.getAttribute("y1")),
        y2: Number(l.getAttribute("y2")),
      })),
    };
    const leaders = [...container.querySelectorAll("polyline[data-leader]")].map((p) =>
      pairs(p.getAttribute("points")),
    );
    expect(leaders.length).toBeGreaterThan(0);
    const hits = (box: ReturnType<typeof cornerBox>) =>
      collisions(box, obstacles) +
      leaders.filter((pts) =>
        pts.some((_, i) => i > 0 && segmentHitsBox(pts[i - 1] as Point, pts[i] as Point, box)),
      ).length;
    const corners = [...container.querySelectorAll("[data-corner]")];
    expect(corners).toHaveLength(2);
    for (const c of corners) {
      const corner = c.getAttribute("data-corner") as "top" | "bottom";
      const side = c.getAttribute("data-side") as "left" | "right";
      const w = cornerWidth(c.textContent ?? "");
      const here = hits(cornerBox(corner, side, w, 326, PLOT_H));
      if (side === "right") expect(here).toBe(0);
      else if (here > 0)
        expect(hits(cornerBox(corner, "right", w, 326, PLOT_H))).toBeGreaterThan(0);
    }
  });

  it("draws the opponent's marks as pencil hairlines and the player's in pen", async () => {
    const { container } = await renderGraph({ playerColor: "white" });
    const plies = games.middlegameBlunder.plies;
    const serious = (c: string) =>
      plies.filter((p) => p.color === c && (p.judgement === "mistake" || p.judgement === "blunder"))
        .length;
    expect(container.querySelectorAll('[data-mark="pen"]')).toHaveLength(serious("white"));
    const opponent = container.querySelectorAll('line[data-mark="opponent"]');
    expect(opponent).toHaveLength(plies.filter((p) => p.color === "black" && p.judgement).length);
    for (const l of opponent) {
      expect(l).toHaveAttribute("stroke-width", "1");
      expect(l).toHaveClass("stroke-pencil");
    }
    for (const label of container.querySelectorAll("[data-label]")) {
      const ply = Number(label.getAttribute("data-label"));
      if (ply % 2 === 0) expect(label.querySelector("tspan")).not.toHaveClass("fill-pen-text");
    }
  });

  it("renders 400 plies inside the width", async () => {
    const points: EvalGraphPoint[] = Array.from({ length: 400 }, (_, i) => ({
      ply: i + 1,
      winWhite: 50 + 45 * Math.sin(i / 17),
      san: i % 2 === 0 ? "Nf3" : "Nf6",
      judgement: i % 50 === 7 ? "blunder" : null,
    }));
    const { container } = await renderGraph({ points });
    expect(container.querySelector("svg")).toHaveAttribute("width", String(WIDTH));
    const lines = container.querySelectorAll('[data-series="line"]');
    expect(lines).toHaveLength(1);
    const segments = (lines[0]?.getAttribute("d") ?? "").split(/(?=[MC])/);
    expect(segments).toHaveLength(400);
    const vertices = segments.map((s) => {
      const n = numbersOf(s.slice(1));
      return n.length === 2 ? n[0] : n[4];
    });
    vertices.forEach((x, i) => {
      expect(x).toBeCloseTo(8 + xForPly(i + 1, WIDTH - 16, 400), 2); // d3-path keeps 3 decimals
      if (i > 0) expect(x).toBeGreaterThan(vertices[i - 1] ?? 0);
    });
    for (const s of segments) {
      numbersOf(s.slice(1))
        .filter((_, i) => i % 2 === 0)
        .forEach((x) => {
          expect(x).toBeGreaterThanOrEqual(0);
          expect(x).toBeLessThanOrEqual(WIDTH);
        });
    }
    const ticks = container.querySelectorAll('[data-mark="pen"]');
    expect(ticks).toHaveLength(8);
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuemax", "400");
  });

  it("draws a one-ply game in the middle", async () => {
    const points: EvalGraphPoint[] = [{ ply: 1, winWhite: 55, san: "e4", judgement: "inaccuracy" }];
    const { container } = await renderGraph({ points });
    expect(container.querySelector('[data-series="line"]')?.getAttribute("d")).not.toMatch(/NaN/);
    expect(container.querySelector('[data-mark="pencil"] line')).toHaveAttribute("x1", "160");
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuemin", "1");
    expect(slider).toHaveAttribute("aria-valuemax", "1");
  });

  it("shows an empty state with no moves, at the populated height", async () => {
    const { container } = await renderGraph({ points: [] });
    expect(screen.getByText("No moves to graph yet")).toBeInTheDocument();
    expect(screen.queryByRole("slider")).toBeNull();
    expect(screen.getByText("No moves to graph yet")).toHaveClass(
      "h-(--eval-graph-height)",
      "@graph:h-(--eval-graph-height-wide)",
    );
    const sized = await renderGraph({ points: [], height: 100 });
    expect(sized.getAllByText("No moves to graph yet")[1]).toHaveStyle({ height: "140px" });
    expect(container).toBeTruthy();
  });
});
