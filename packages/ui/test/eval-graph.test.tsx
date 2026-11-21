import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { EvalGraph, type EvalGraphPoint, xForPly, yForWin } from "../src/eval-graph/index.js";
import { games } from "../src/fixtures/index.js";

const WIDTH = 320;

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
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    get: () => WIDTH,
  });
});

afterEach(cleanup);

const fixturePoints = (): EvalGraphPoint[] =>
  games.middlegameBlunder.plies.map((p) => ({
    ply: p.ply,
    winWhite: p.winWhite,
    san: p.san,
    judgement: p.judgement,
  }));

async function renderGraph(props: Partial<Parameters<typeof EvalGraph>[0]> = {}) {
  let result: ReturnType<typeof render> | undefined;
  await act(async () => {
    result = render(<EvalGraph points={fixturePoints()} label="Win chance" {...props} />);
  });
  // useParentSize applies the measured width on the next animation frame.
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });
  return result as ReturnType<typeof render>;
}

describe("scales", () => {
  it("maps win percent to y with White at the top", () => {
    expect(yForWin(50, 160)).toBe(80);
    expect(yForWin(100, 160)).toBe(0);
    expect(yForWin(0, 160)).toBe(160);
  });

  it("spreads plies across the width", () => {
    expect(xForPly(1, 320, 91)).toBe(0);
    expect(xForPly(91, 320, 91)).toBe(320);
    expect(xForPly(1, 320, 1)).toBe(160);
  });
});

describe("EvalGraph", () => {
  it("draws one pen tick per mistake or blunder", async () => {
    const { container } = await renderGraph();
    const serious = games.middlegameBlunder.plies.filter(
      (p) => p.judgement === "mistake" || p.judgement === "blunder",
    ).length;
    expect(serious).toBeGreaterThan(0);
    expect(container.querySelectorAll('[data-mark="pen"]')).toHaveLength(serious);
    const inaccuracies = games.middlegameBlunder.plies.filter(
      (p) => p.judgement === "inaccuracy",
    ).length;
    expect(container.querySelectorAll('[data-mark="pencil"]')).toHaveLength(inaccuracies);
  });

  it("steps plies from the keyboard", async () => {
    const onSelectPly = vi.fn();
    await renderGraph({ currentPly: 30, onSelectPly });
    const slider = screen.getByRole("slider", { name: "Win chance" });
    expect(slider).toHaveAttribute("aria-valuemin", "1");
    expect(slider).toHaveAttribute("aria-valuemax", String(games.middlegameBlunder.plies.length));
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(onSelectPly).toHaveBeenLastCalledWith(31);
    fireEvent.keyDown(slider, { key: "End" });
    expect(onSelectPly).toHaveBeenLastCalledWith(games.middlegameBlunder.plies.length);
    fireEvent.keyDown(slider, { key: "Home" });
    expect(onSelectPly).toHaveBeenLastCalledWith(1);
  });

  it("describes the current ply in words", async () => {
    await renderGraph({ currentPly: 31 });
    const ply = games.middlegameBlunder.plies[30];
    if (!ply) throw new Error("fixture has no ply 31");
    const side = ply.ply % 2 === 1 ? "White" : "Black";
    const parts = [`Move ${Math.ceil(ply.ply / 2)}`, `${side} ${ply.san}`];
    if (ply.judgement) parts.push(ply.judgement);
    parts.push(`White ${Math.round(ply.winWhite)}%`);
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuetext", parts.join(", "));
  });

  it("renders 400 plies", async () => {
    const points: EvalGraphPoint[] = Array.from({ length: 400 }, (_, i) => ({
      ply: i + 1,
      winWhite: 50 + 45 * Math.sin(i / 17),
      san: i % 2 === 0 ? "Nf3" : "Nf6",
      judgement: i % 50 === 7 ? "blunder" : null,
    }));
    const { container } = await renderGraph({ points });
    const lines = container.querySelectorAll('[data-series="line"]');
    expect(lines).toHaveLength(1);
    const d = lines[0]?.getAttribute("d") ?? "";
    const xs = [...d.matchAll(/(-?\d+(?:\.\d+)?)[ ,](-?\d+(?:\.\d+)?)/g)].map((m) => Number(m[1]));
    expect(xs.length).toBeGreaterThanOrEqual(400);
    for (const x of xs) {
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x).toBeLessThanOrEqual(WIDTH);
    }
  });

  it("shows an empty state with no moves", async () => {
    await renderGraph({ points: [] });
    expect(screen.getByText("No moves to graph yet")).toBeInTheDocument();
    expect(screen.queryByRole("slider")).toBeNull();
  });
});
