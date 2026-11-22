import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  AccuracyTrend,
  type AccuracyTrendProps,
  JudgementColumns,
  ResultSplit,
  resultText,
  TREND_PLOT_H,
  TREND_TOP,
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
    expect(container.querySelector("clipPath rect")?.getAttribute("width")).toBe("640");
  });

  it("starts the clip rect at zero width when drawing in", async () => {
    stubMedia();
    const { container } = await renderTrend({ points: points(["win", "loss"]), drawIn: true });
    expect(container.querySelector("clipPath rect")?.getAttribute("width")).toBe("0");
  });

  it("clips the band and the mean line to the reveal", async () => {
    const pts = points(["win", "win", "loss"]);
    const rolling = pts.map((p) => ({ id: p.id, mean: 70, low: 65, high: 75 }));
    const { container } = await renderTrend({ points: pts, rolling });
    for (const part of ["band", "mean"]) {
      const el = container.querySelector(`[data-part="${part}"]`);
      expect(el?.closest("g[clip-path]")).not.toBeNull();
    }
  });

  it("puts higher accuracy higher, with 0% and 100% on the plot's edges", async () => {
    const pts = points(["win", "win", "win", "win"]);
    const acc = [90, 50, 0, 100];
    pts.forEach((p, i) => {
      p.accuracy = acc[i] as number;
    });
    const { container } = await renderTrend({ points: pts });
    const cy = (id: string) =>
      Number(container.querySelector(`[data-id="${id}"]`)?.getAttribute("data-cy"));
    expect(cy("g0")).toBeLessThan(cy("g1"));
    expect(cy("g3")).toBe(TREND_TOP);
    expect(cy("g2")).toBe(TREND_TOP + TREND_PLOT_H);
  });

  it("orders games by date whatever the input order", async () => {
    const pts = points(["win", "loss", "draw", "win"]).reverse();
    const { container } = await renderTrend({ points: pts });
    const ids = [...container.querySelectorAll("[data-result]")].map((m) =>
      m.getAttribute("data-id"),
    );
    expect(ids).toEqual(["g0", "g1", "g2", "g3"]);
    const xs = [...container.querySelectorAll("[data-result]")].map((m) =>
      Number(m.getAttribute("data-cx")),
    );
    expect([...xs].sort((a, b) => a - b)).toEqual(xs);
    const first = screen.getByRole("button", { name: "Game 1, win" });
    first.focus();
    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(screen.getByRole("button", { name: "Game 2, loss" })).toHaveFocus();
  });

  it("draws a loss as an ink-soft ring", async () => {
    const { container } = await renderTrend({ points: points(["win", "loss"]) });
    const ring = container.querySelector('[data-result="loss"] [data-part="ring"]');
    expect(ring?.getAttribute("class")).toContain("stroke-ink-soft");
  });

  it("draws no band where an interval end is null, and keeps the mean", async () => {
    const pts = points(["win", "win", "loss", "draw", "win", "loss"]);
    const rolling = pts.map((p, i) => ({
      id: p.id,
      mean: 70,
      low: i === 2 || i === 3 ? null : 65,
      high: i === 3 ? null : 75,
    }));
    const { container } = await renderTrend({ points: pts, rolling });
    const bands = [...container.querySelectorAll('[data-part="band"]')];
    expect(container.querySelectorAll('[data-part="mean"]')).toHaveLength(1);
    expect(bands).toHaveLength(2);
    // Never a band along 0%: the plot's bottom edge.
    for (const b of bands) {
      expect(b.getAttribute("d")).not.toContain(`,${TREND_TOP + TREND_PLOT_H}`);
    }
  });

  it("leaves out a game whose accuracy is not a number, and says so", async () => {
    const pts = points(["win", "loss", "draw"]);
    (pts[1] as Point).accuracy = Number.NaN;
    const { container } = await renderTrend({ points: pts });
    expect(container.querySelectorAll("[data-result]")).toHaveLength(2);
    expect(container.querySelectorAll('[data-result="loss"]')).toHaveLength(0);
    expect(screen.queryByText(/Loss \d/)).toBeNull();
    expect(screen.getByText("1 game has no accuracy and isn't shown.")).toBeInTheDocument();
  });

  // T4 parked minor, settled in T13: a bad date is not "no accuracy".
  it("says which games it left out for a bad date and which for no accuracy", async () => {
    const pts = points(["win", "loss", "draw", "win"]);
    (pts[0] as Point).date = "not a date";
    (pts[1] as Point).accuracy = Number.NaN;
    (pts[2] as Point).date = "2026-13-45";
    await renderTrend({ points: pts, minPoints: 1 });
    expect(screen.getByText("2 games have no valid date and aren't shown.")).toBeInTheDocument();
    expect(screen.getByText("1 game has no accuracy and isn't shown.")).toBeInTheDocument();
  });

  // T4 parked minor, settled in T13: the legend names only what is drawn.
  it("keys the band only when a band is drawn", async () => {
    const pts = points(["win", "loss", "draw", "win"]);
    const noBand = pts.map((p) => ({ id: p.id, mean: 70, low: null, high: null }));
    const { unmount } = await renderTrend({ points: pts, rolling: noBand });
    expect(screen.getByText("Rolling average")).toBeInTheDocument();
    expect(screen.queryByText(/80% band/)).toBeNull();
    unmount();
    const band = pts.map((p) => ({ id: p.id, mean: 70, low: 65, high: 75 }));
    await renderTrend({ points: pts, rolling: band });
    expect(screen.getByText("Rolling average, 80% band")).toBeInTheDocument();
  });

  it("summarises the trend for screen readers", async () => {
    const pts = points(["win", "loss", "draw"]);
    const rolling = [{ id: "g2", mean: 71.26, low: 68.04, high: 74.4 }];
    await renderTrend({ points: pts, rolling });
    expect(
      screen.getByText(
        "3 games, 1 Jan 2026 to 3 Jan 2026. Rolling average 71.3% on 3 Jan 2026, 80% likely between 68.0% and 74.4%.",
      ),
    ).toBeInTheDocument();
  });

  it("tones dots down and keeps them small when they are dense", async () => {
    measured = 320;
    stubMedia({ coarse: true });
    const { container } = await renderTrend({
      points: points(Array.from({ length: 500 }, () => "win" as Result)),
    });
    const fill = container.querySelector('[data-result="win"] [data-part="fill"]');
    expect(fill?.getAttribute("r")).toBe("3");
    expect(fill?.getAttribute("class")).toContain("fill-ink-soft");
  });

  it("leaves pointer hits to one overlay, not the dot buttons", async () => {
    await renderTrend({ points: points(["win", "loss"]) });
    for (const b of screen.getAllByRole("button")) {
      expect(b.className).toContain("pointer-events-none");
    }
  });

  it("opens the nearest game to a tap, then steps with Previous and Next", async () => {
    stubMedia({ coarse: true });
    const { container } = await renderTrend({ points: points(["win", "loss", "draw"]) });
    const mark = container.querySelector('[data-id="g1"]');
    const x = Number(mark?.getAttribute("data-cx"));
    const y = Number(mark?.getAttribute("data-cy"));
    const overlay = container.querySelector('[data-part="hit"]') as HTMLElement;
    fireEvent.click(overlay, { clientX: x + 8, clientY: y + 6 });
    expect(await screen.findByText("Game 2, loss", { selector: "p" })).toBeInTheDocument();
    expect(container.querySelector('[data-part="selected"]')).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Next game" }));
    expect(await screen.findByText("Game 3, draw", { selector: "p" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next game" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Previous game" }));
    fireEvent.click(screen.getByRole("button", { name: "Previous game" }));
    expect(await screen.findByText("Game 1, win", { selector: "p" })).toBeInTheDocument();
  });

  it("ignores a tap far from every game", async () => {
    const { container } = await renderTrend({ points: points(["win", "loss"]) });
    const overlay = container.querySelector('[data-part="hit"]') as HTMLElement;
    fireEvent.click(overlay, { clientX: 600, clientY: 190 });
    expect(screen.queryByRole("link", { name: "Review game" })).toBeNull();
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

describe("JudgementColumns, honesty and access", () => {
  it("says so when there are no games, with no plot or legend", () => {
    const { container } = render(<JudgementColumns games={[]} label="Mistakes" />);
    expect(screen.getByText("No analysed games yet")).toBeInTheDocument();
    expect(container.querySelector("svg")).toBeNull();
    expect(screen.queryByText("Blunder")).toBeNull();
  });

  it("describes each column's counts and the total for screen readers", () => {
    render(<JudgementColumns games={[game(1, 2, 1, 3), game(2, 0, 0, 0)]} label="Mistakes" />);
    const link = screen.getByRole("link", { name: "Game 1" });
    expect(link).toHaveAccessibleDescription("2 blunders, 1 mistake, 3 inaccuracies");
    expect(screen.getByRole("link", { name: "Game 2" })).toHaveAccessibleDescription(
      "no marked moves",
    );
    expect(screen.getByText("2 games: 2 blunders, 1 mistake, 3 inaccuracies.")).toBeInTheDocument();
  });

  it("is one tab stop, starting on the latest game, with arrow keys between games", () => {
    render(
      <JudgementColumns
        games={[game(1, 1, 0, 0), game(2, 0, 1, 0), game(3, 0, 0, 1)]}
        label="Mistakes"
      />,
    );
    const links = screen.getAllByRole("link");
    expect(links.filter((l) => l.getAttribute("tabindex") === "0")).toHaveLength(1);
    const latest = screen.getByRole("link", { name: "Game 3" });
    expect(latest).toHaveAttribute("tabindex", "0");
    latest.focus();
    fireEvent.keyDown(latest, { key: "ArrowLeft" });
    expect(screen.getByRole("link", { name: "Game 2" })).toHaveFocus();
    fireEvent.keyDown(document.activeElement as Element, { key: "Home" });
    expect(screen.getByRole("link", { name: "Game 1" })).toHaveFocus();
  });

  // M5a T13 critic I1: right-aligned columns left the heading's side of the row empty at 1440.
  // They start under the heading, oldest first, so the latest game is the rightmost column.
  it("starts the columns at the left edge, the latest game rightmost", async () => {
    measured = 900;
    render(<JudgementColumns games={[game(1, 1, 0, 0), game(2, 0, 1, 0)]} label="Mistakes" />);
    await nextFrame();
    const first = screen.getByRole("link", { name: "Game 1" });
    const last = screen.getByRole("link", { name: "Game 2" });
    expect(Number(first.getAttribute("data-x"))).toBe(0);
    expect(Number(last.getAttribute("data-x"))).toBeGreaterThan(
      Number(first.getAttribute("data-x")),
    );
  });

  it("opens scrolled to the latest games", async () => {
    measured = 320;
    const set = vi.fn();
    const proto = HTMLElement.prototype;
    Object.defineProperty(proto, "scrollWidth", { configurable: true, get: () => 480 });
    Object.defineProperty(proto, "scrollLeft", { configurable: true, get: () => 0, set });
    try {
      const games = Array.from({ length: 40 }, (_, i) => game(i, 1, 0, 0));
      render(<JudgementColumns games={games} label="Mistakes" />);
      await nextFrame();
      expect(set).toHaveBeenLastCalledWith(480);
    } finally {
      Reflect.deleteProperty(proto, "scrollWidth");
      Reflect.deleteProperty(proto, "scrollLeft");
    }
  });
});

describe("ResultSplit", () => {
  it("treats a negative or fractional count as whole games only", () => {
    const { container } = render(
      <ResultSplit rows={[{ key: "rapid", label: "Rapid", wins: -2, draws: 1.6, losses: 1 }]} />,
    );
    expect(screen.getByText("0 wins, 1 draw, 1 loss")).toBeInTheDocument();
    expect(container.querySelector('[data-segment="wins"]')).toBeNull();
  });

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
