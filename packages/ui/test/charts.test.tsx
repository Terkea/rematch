import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  AccuracyTrend,
  type AccuracyTrendProps,
  JudgementColumns,
  ResultSplit,
  resultText,
} from "../src/charts/index.js";

/** The width the ResizeObserver stub reports. */
let measured = 640;

// jsdom has no layout: report every observed element as `measured` wide.
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

function stubMedia({ reduce = false, coarse = false } = {}) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: (reduce && query.includes("reduce")) || (coarse && query.includes("coarse")),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

beforeAll(() => {
  vi.stubGlobal("ResizeObserver", FixedResizeObserver);
});

afterEach(() => {
  cleanup();
  measured = 640;
  vi.unstubAllGlobals();
  vi.stubGlobal("ResizeObserver", FixedResizeObserver);
});

type Point = AccuracyTrendProps["points"][number];
type Result = Point["result"];

const DAY = 86_400_000;
const START = Date.UTC(2026, 0, 1);

/** Synthetic rows, allowed in tests (B15 covers UI). */
function points(results: Result[]): Point[] {
  return results.map((result, i) => ({
    id: `g${i}`,
    date: new Date(START + i * DAY).toISOString(),
    accuracy: 60 + ((i * 7) % 30),
    result,
    caption: `Game ${i + 1}, ${result}`,
    href: `/app/games/g${i}`,
  }));
}

/** useParentSize applies the measured width on the next animation frame. */
const nextFrame = () =>
  act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });

async function renderTrend(props: Partial<AccuracyTrendProps> & { points: Point[] }) {
  const view = render(<AccuracyTrend rolling={[]} label="Accuracy over time" {...props} />);
  await nextFrame();
  return view;
}

describe("AccuracyTrend", () => {
  it("draws each result as its own shape", async () => {
    const { container } = await renderTrend({
      points: points(["win", "win", "draw", "win", "draw", "loss"]),
    });
    expect(container.querySelectorAll('[data-result="win"]')).toHaveLength(3);
    expect(container.querySelectorAll('[data-result="draw"]')).toHaveLength(2);
    expect(container.querySelectorAll('[data-result="loss"]')).toHaveLength(1);
    // Shape, not colour: a win is a filled disc, a draw has a half fill, a loss is a ring.
    expect(container.querySelector('[data-result="win"] [data-part="fill"]')).not.toBeNull();
    expect(container.querySelector('[data-result="draw"] [data-part="half"]')).not.toBeNull();
    expect(container.querySelector('[data-result="loss"] [data-part="ring"]')).not.toBeNull();
    expect(container.querySelector('[data-result="loss"] [data-part="fill"]')).toBeNull();
  });

  it("draws an unfinished game as a pencil cross", async () => {
    const { container } = await renderTrend({ points: points(["win", "unfinished"]) });
    const cross = container.querySelector('[data-result="unfinished"] [data-part="cross"]');
    expect(cross).not.toBeNull();
    expect(cross?.getAttribute("class")).toContain("stroke-pencil");
  });

  it("labels the SVG and gives the y axis ticks at 25, 50 and 75", async () => {
    await renderTrend({ points: points(["win", "loss"]) });
    const svg = screen.getByRole("img", { name: "Accuracy over time" });
    expect(svg.tagName.toLowerCase()).toBe("svg");
    const ticks = [...svg.querySelectorAll("[data-tick-y]")].map((t) =>
      t.getAttribute("data-tick-y"),
    );
    expect(ticks).toEqual(["25", "50", "75"]);
  });

  it("shows text instead of a chart with fewer than two games", async () => {
    const { container } = await renderTrend({ points: points(["win"]) });
    expect(screen.getByText("Not enough games yet")).toBeInTheDocument();
    expect(container.querySelector("svg")).toBeNull();
  });

  it("breaks the rolling mean where a game has no entry", async () => {
    const pts = points(["win", "win", "loss", "draw", "win", "loss"]);
    const rolling = [0, 1, 3, 4, 5].map((i) => ({
      id: `g${i}`,
      mean: 70,
      low: 65,
      high: 75,
    }));
    const { container } = await renderTrend({ points: pts, rolling });
    expect(container.querySelectorAll('[data-part="mean"]')).toHaveLength(2);
    expect(container.querySelectorAll('[data-part="band"]')).toHaveLength(2);
  });

  it("keeps 500 dots inside a 320px container", async () => {
    measured = 320;
    const results: Result[] = ["win", "draw", "loss", "unfinished"];
    const { container } = await renderTrend({
      points: points(Array.from({ length: 500 }, (_, i) => results[i % 4] as Result)),
    });
    const marks = [...container.querySelectorAll("[data-result]")];
    expect(marks).toHaveLength(500);
    for (const m of marks) {
      const cx = Number(m.getAttribute("data-cx"));
      expect(cx).toBeGreaterThanOrEqual(0);
      expect(cx).toBeLessThanOrEqual(320);
    }
  });

  it("opens a dot's caption with a link to review the game", async () => {
    stubMedia();
    await renderTrend({ points: points(["win", "loss", "draw"]) });
    fireEvent.click(screen.getByRole("button", { name: "Game 2, loss" }));
    const link = await screen.findByRole("link", { name: "Review game" });
    expect(link).toHaveAttribute("href", "/app/games/g1");
    expect(screen.getByText("Game 2, loss", { selector: "p" })).toBeInTheDocument();
  });

  it("moves between games with the arrow keys, one tab stop for the chart", async () => {
    await renderTrend({ points: points(["win", "loss", "draw"]) });
    const buttons = screen.getAllByRole("button");
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    const last = screen.getByRole("button", { name: "Game 3, draw" });
    expect(last.tabIndex).toBe(0);
    last.focus();
    fireEvent.keyDown(last, { key: "ArrowLeft" });
    expect(screen.getByRole("button", { name: "Game 2, loss" })).toHaveFocus();
    fireEvent.keyDown(document.activeElement as Element, { key: "Home" });
    expect(screen.getByRole("button", { name: "Game 1, win" })).toHaveFocus();
  });

  it("uses radius 3, or 4.5 on a coarse pointer", async () => {
    stubMedia();
    const fine = await renderTrend({ points: points(["win", "loss"]) });
    expect(fine.container.querySelector('[data-part="fill"]')?.getAttribute("r")).toBe("3");
    cleanup();
    stubMedia({ coarse: true });
    const coarse = await renderTrend({ points: points(["win", "loss"]) });
    expect(coarse.container.querySelector('[data-part="fill"]')?.getAttribute("r")).toBe("4.5");
  });

  it("sweeps a clip in from zero when drawing in", async () => {
    stubMedia();
    const { container } = await renderTrend({ points: points(["win", "loss"]), drawIn: true });
    const svg = container.querySelector("svg[role='img']") as SVGElement;
    expect(svg.style.getPropertyValue("--draw")).toBe("0");
  });

  it("has no clip animation under reduced motion", async () => {
    stubMedia({ reduce: true });
    const { container } = await renderTrend({ points: points(["win", "loss"]), drawIn: true });
    const svg = container.querySelector("svg[role='img']") as SVGElement;
    // The clip rect is at its final width on the first render.
    expect(svg.style.getPropertyValue("--draw")).toBe("1");
    const rect = container.querySelector("clipPath rect");
    expect(rect?.getAttribute("width")).toBe(String(640));
  });
});

const game = (
  i: number,
  blunders: number,
  mistakes: number,
  inaccuracies: number,
): {
  id: string;
  label: string;
  href: string;
  blunders: number;
  mistakes: number;
  inaccuracies: number;
} => ({
  id: `g${i}`,
  label: `Game ${i}`,
  href: `/app/games/g${i}`,
  blunders,
  mistakes,
  inaccuracies,
});

describe("JudgementColumns", () => {
  it("stacks one cell per move: blunders, then mistakes, then inaccuracies", () => {
    const { container } = render(
      <JudgementColumns games={[game(1, 2, 1, 3)]} label="Mistakes in your last 40 games" />,
    );
    const blunders = container.querySelectorAll('[data-cell="blunder"]');
    const mistakes = container.querySelectorAll('[data-cell="mistake"]');
    const inaccuracies = container.querySelectorAll('[data-cell="inaccuracy"]');
    expect(blunders).toHaveLength(2);
    expect(mistakes).toHaveLength(1);
    expect(inaccuracies).toHaveLength(3);
    for (const b of blunders) expect(b.getAttribute("class")).toContain("fill-pen");
    expect(mistakes[0]?.getAttribute("fill")).toMatch(/^url\(#.*hatch-pen\)$/);
    for (const c of inaccuracies) expect(c.getAttribute("class")).toContain("stroke-pencil");
    // From the baseline up: each later cell sits higher (smaller y).
    const ys = [...blunders, ...mistakes, ...inaccuracies].map((c) => Number(c.getAttribute("y")));
    expect([...ys].sort((a, b) => b - a)).toEqual(ys);
    for (const text of ["Blunder", "Mistake", "Inaccuracy"]) {
      expect(screen.getByText(text)).toBeInTheDocument();
    }
  });

  it("links each column to its game, oldest on the left", () => {
    render(<JudgementColumns games={[game(1, 1, 0, 0), game(2, 0, 0, 0)]} label="Mistakes" />);
    const first = screen.getByRole("link", { name: "Game 1" });
    const second = screen.getByRole("link", { name: "Game 2" });
    expect(first).toHaveAttribute("href", "/app/games/g1");
    expect(Number(first.getAttribute("data-x"))).toBeLessThan(
      Number(second.getAttribute("data-x")),
    );
    // A clean game is a 1px rule tick, not an empty gap.
    const tick = second.querySelector('[data-cell="none"]');
    expect(tick?.getAttribute("class")).toContain("fill-rule");
    expect(tick?.getAttribute("height")).toBe("1");
    expect(screen.getByRole("figure", { name: "Mistakes" })).toBeInTheDocument();
  });

  it("scrolls inside its own container below 480px", async () => {
    measured = 320;
    const games = Array.from({ length: 40 }, (_, i) => game(i, 1, 1, 1));
    const { container } = render(<JudgementColumns games={games} label="Mistakes" />);
    await nextFrame();
    const scroller = container.querySelector("[data-part='scroller']");
    expect(scroller?.className).toContain("overflow-x-auto");
    expect(
      Number(container.querySelector("[data-part='scroller'] > svg")?.getAttribute("width")),
    ).toBeGreaterThanOrEqual(480);
  });
});

describe("ResultSplit", () => {
  it("states the split in words and sizes the win segment", () => {
    const { container } = render(
      <ResultSplit rows={[{ key: "rapid", label: "Rapid", wins: 8, draws: 2, losses: 4 }]} />,
    );
    expect(screen.getByText("8 wins, 2 draws, 4 losses")).toBeInTheDocument();
    const win = container.querySelector<HTMLElement>('[data-segment="wins"]');
    expect(Math.abs(Number.parseFloat(win?.style.width ?? "") - 57.14)).toBeLessThanOrEqual(0.5);
    expect(win?.querySelector("[data-fill]")?.className).toContain("bg-ink");
    const draws = container.querySelector('[data-segment="draws"] rect');
    expect(draws?.getAttribute("fill")).toMatch(/^url\(#.*hatch-ink\)$/);
    const losses = container.querySelector('[data-segment="losses"] [data-fill]');
    expect(losses?.className).toContain("bg-sheet");
  });

  it("uses singular forms", () => {
    render(<ResultSplit rows={[{ key: "blitz", label: "Blitz", wins: 1, draws: 0, losses: 1 }]} />);
    expect(screen.getByText("1 win, 0 draws, 1 loss")).toBeInTheDocument();
    expect(resultText(0, 1, 0)).toBe("0 wins, 1 draw, 0 losses");
  });

  it("skips a row with no games", () => {
    render(
      <ResultSplit
        rows={[
          { key: "rapid", label: "Rapid", wins: 1, draws: 1, losses: 1 },
          { key: "daily", label: "Daily", wins: 0, draws: 0, losses: 0 },
        ]}
      />,
    );
    expect(screen.queryByText("Daily")).toBeNull();
    expect(screen.getByText("Rapid")).toBeInTheDocument();
  });
});
