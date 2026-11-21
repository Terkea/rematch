import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  EvalGraph,
  type EvalGraphPoint,
  type EvalGraphProps,
  type LabelMark,
  placeLabels,
  wipeDelay,
  xForPly,
  yForWin,
} from "../src/eval-graph/index.js";
import { games } from "../src/fixtures/index.js";

const WIDTH = 320;
// At 320px the container is narrow: an 88px plot under a 22px label lane.
const LANE = 22;
const PLOT_H = 88;

// jsdom has no layout: report every observed element as WIDTH wide.
class FixedResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element) {
    const rect = { width: WIDTH, height: 0, top: 0, left: 0, right: WIDTH, bottom: 0, x: 0, y: 0 };
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

afterEach(cleanup);

const fixturePoints = (): EvalGraphPoint[] =>
  games.middlegameBlunder.plies.map((p) => ({
    ply: p.ply,
    winWhite: p.winWhite,
    san: p.san,
    judgement: p.judgement,
  }));

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

  it("times the wipe by the pen easing", () => {
    expect(wipeDelay(0, 1.2, [0.6, 0.05, 0.3, 1])).toBeCloseTo(0, 5);
    expect(wipeDelay(1, 1.2, [0.6, 0.05, 0.3, 1])).toBeCloseTo(1.2, 5);
    const at = [0.25, 0.5, 0.75].map((p) => wipeDelay(p, 1.2, [0.6, 0.05, 0.3, 1]));
    expect(at[0]).toBeLessThan(at[1] ?? 0);
    expect(at[1]).toBeLessThan(at[2] ?? 0);
    // The pen's slow start: the first quarter of the width takes longer than a quarter of the time.
    expect(at[0]).toBeGreaterThan(0.3);
  });
});

const mk = (
  ply: number,
  san: string,
  judgement: LabelMark["judgement"],
  x: number,
  mine: boolean | null,
): LabelMark => ({ ply, san, judgement, x, mine });

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

  it("falls back to a glyph straight above the tick, with no leader", () => {
    const labels = placeLabels(
      [
        mk(55, "Rxe6", "blunder", 100, true),
        mk(57, "Nf3", "mistake", 230, true),
        mk(60, "exd8=Q+", "blunder", 180, false),
      ],
      320,
      narrow,
    );
    const glyph = labels.find((l) => l.ply === 60);
    expect(glyph?.text).toEqual({ prefix: "", san: "", glyph: "??" });
    expect(glyph?.leader).toBe(false);
    expect(Math.abs((glyph?.x ?? 0) + (glyph?.width ?? 0) / 2 - 180)).toBeLessThanOrEqual(4);
  });

  it("drops a label whose glyph can't sit within 4px of its tick", () => {
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

  it("treats ply 0 as the start position, with no cursor", async () => {
    const onSelectPly = vi.fn();
    const { container } = await renderGraph({ currentPly: 0, onSelectPly });
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuenow", "0");
    expect(slider).toHaveAttribute("aria-valuemin", "0");
    expect(slider).toHaveAttribute("aria-valuetext", "Start position");
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

  it("reveals with a clip, but not under reduced motion", async () => {
    const { container, unmount } = await renderGraph({ drawIn: true });
    expect(container.querySelector("clipPath")).not.toBeNull();
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
    const touch = { pointerType: "touch", pointerId: 7, button: 0 };

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
  });

  it("selects on mouse press, but not on other buttons", async () => {
    const onSelectPly = vi.fn();
    await renderGraph({ currentPly: 30, onSelectPly });
    const slider = screen.getByRole("slider");
    fireEvent.pointerDown(slider, { pointerType: "mouse", pointerId: 1, button: 2, clientX: 200 });
    expect(onSelectPly).not.toHaveBeenCalled();
    fireEvent.pointerDown(slider, { pointerType: "mouse", pointerId: 1, button: 0, clientX: 200 });
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
      fireEvent.pointerMove(slider, { pointerType: "mouse", pointerId: 1, clientX: 200 });
    });
    // Below 512px the readout takes the phase lane, which hides the phase labels meanwhile.
    expect(lane()?.textContent).toMatch(/White \d+%/);
    expect(container.querySelector("[data-readout]")).not.toBeNull();
    await act(async () => {
      fireEvent.pointerLeave(slider, { pointerType: "mouse", pointerId: 1 });
    });
    expect(lane()).toBeNull();
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
    expect(screen.getByText("No moves to graph yet")).toHaveClass("h-32", "@lg:h-40");
    const sized = await renderGraph({ points: [], height: 100 });
    expect(sized.getAllByText("No moves to graph yet")[1]).toHaveStyle({ height: "140px" });
    expect(container).toBeTruthy();
  });
});
