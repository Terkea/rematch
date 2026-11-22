import "@testing-library/jest-dom/vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  AccuracyTrend,
  type AccuracyTrendProps,
  dateTicks,
  ResultSplit,
} from "../src/charts/index.js";
import { timeScale } from "../src/charts/scales.js";
import { bandText as bandTextPlain } from "../src/stats/band.js";
import { bandText, KpiStat } from "../src/stats/index.js";

// Task 8 (overview page) hand-offs that live in @ci/ui: a server-safe bandText, an honest KPI row
// with no arbitrary ruler, sparkline ends with their direction, a trend line that breaks across
// weeks with no games, the first-view number roll, and dark-mode weight. Synthetic values only.

const measured = 640;
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
  vi.useRealTimers();
});

const kpi = {
  label: "Blunders",
  value: 2.41,
  format: "decimal" as const,
  interval: { low: 2.2, high: 2.6, level: 0.8 as const },
  basis: { games: 120, timeClass: "analysed" },
};

describe("bandText is server-safe", () => {
  it("lives in a module with no 'use client' directive, and the barrel re-exports it", () => {
    const src = readFileSync(join(process.cwd(), "src/stats/band.ts"), "utf8");
    expect(src).not.toMatch(/^\s*["']use client["']/);
    expect(bandText).toBe(bandTextPlain);
    expect(bandText(kpi.interval, "decimal")).toBe("80% likely between 2.2 and 2.6");
  });
});

describe("KpiStat honest options", () => {
  it("drops the ruler with scale={false} but keeps the band in words", () => {
    const { container } = render(<KpiStat {...kpi} scale={false} />);
    expect(container.querySelector('[data-part="band"]')).toBeNull();
    expect(container.querySelector('[data-part="value"]')).toBeNull();
    expect(screen.getByText("80% likely between 2.2 and 2.6")).toBeInTheDocument();
  });

  it("keeps the basis for screen readers only when basisHidden", () => {
    render(<KpiStat {...kpi} basisHidden />);
    const basis = screen.getByText("Based on 120 analysed games");
    expect(basis).toHaveClass("sr-only");
  });

  it("labels the sparkline's first and last value and which way is better", () => {
    const { container } = render(
      <KpiStat {...kpi} trend={[2.8, null, 2.5, 2.3]} trendEnds better="lower" />,
    );
    const ends = container.querySelector("[data-part='trend-ends']");
    expect(ends?.textContent).toContain("2.8");
    expect(ends?.textContent).toContain("2.3");
    expect(ends?.textContent).toContain("Lower is better");
    // Never a verdict: the words name the convention, not "improved" or "worse".
    expect(container.textContent).not.toMatch(/improved|worse/i);
  });

  // M5a T13 critic I3: bare "1.0 … 4.0" beside "1.50" read as a contradiction. With the games
  // behind each point, the ends say which weeks they are and how many games each rests on.
  it("names the end weeks and their game counts when it has them", () => {
    const { container } = render(
      <KpiStat
        {...kpi}
        trend={[null, 1, 2.5, null, 4]}
        trendGames={[0, 2, 3, 0, 1]}
        trendEnds
        better="lower"
      />,
    );
    const ends = container.querySelector("[data-part='trend-ends']");
    expect(ends?.textContent).toContain("First week 1.0 · 2 games");
    expect(ends?.textContent).toContain("Latest week 4.0 · 1 game");
    // Read out too, not only drawn.
    expect(ends?.querySelector("[aria-hidden='true']")).toBeNull();
  });
});

const DAY = 86_400_000;
type Point = AccuracyTrendProps["points"][number];

describe("AccuracyTrend gapDays", () => {
  it("breaks the rolling line and band across a span with no games", async () => {
    // 12 games a day apart, 90 days with none, then 12 more.
    const dates = [
      ...Array.from({ length: 12 }, (_, i) => i),
      ...Array.from({ length: 12 }, (_, i) => 102 + i),
    ];
    const points: Point[] = dates.map((d, i) => ({
      id: `g${i}`,
      date: new Date(Date.UTC(2026, 0, 1) + d * DAY).toISOString(),
      accuracy: 60,
      result: "win",
      caption: `Game ${i}`,
      href: `/app/games/g${i}`,
    }));
    const rolling = points.map((p) => ({ id: p.id, mean: 60, low: 55, high: 65 }));
    const view = render(
      <AccuracyTrend points={points} rolling={rolling} label="Accuracy" gapDays={28} />,
    );
    await act(async () => {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(view.container.querySelectorAll('[data-part="mean"]')).toHaveLength(2);
    expect(view.container.querySelectorAll('[data-part="band"]')).toHaveLength(2);
    view.unmount();

    // Without gapDays the line bridges the gap (the chart's default is unchanged).
    const bridged = render(<AccuracyTrend points={points} rolling={rolling} label="Accuracy" />);
    await act(async () => {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(bridged.container.querySelectorAll('[data-part="mean"]')).toHaveLength(1);
  });
});

describe("AccuracyTrend minPoints", () => {
  const one: Point[] = [
    {
      id: "g0",
      date: new Date(Date.UTC(2026, 0, 5)).toISOString(),
      accuracy: 61,
      result: "win",
      caption: "Game 0",
      href: "/app/games/g0",
    },
  ];

  it("plots a single game as a dot with minPoints={1}", async () => {
    const view = render(<AccuracyTrend points={one} rolling={[]} label="Accuracy" minPoints={1} />);
    await act(async () => {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(view.container.querySelectorAll("[data-result]")).toHaveLength(1);
    expect(view.container.textContent).not.toContain("Not enough games yet");
  });

  it("labels a one-game span with one tick per day, never two of the same date", () => {
    const at = new Date(Date.UTC(2026, 10, 20, 15));
    const ticks = dateTicks(timeScale([at], 900), 900);
    const days = ticks.map((t) => t.iso);
    expect(new Set(days).size).toBe(days.length);
    expect(days.length).toBeGreaterThan(0);
    // Each label sits on its day's midnight, so the spacing is even.
    expect(days).toEqual(["2026-11-20", "2026-11-21"]);
  });

  // M5a T13 critic I2: a phone-width axis over two months showed only "1 Oct".
  it("labels both ends of the span at every width, with no label crowding an end", () => {
    const first = new Date(Date.UTC(2026, 8, 6, 10));
    const last = new Date(Date.UTC(2026, 9, 31, 18));
    for (const width of [320, 360, 640, 1048]) {
      const ticks = dateTicks(timeScale([first, last], width), width);
      expect(ticks[0], `${width}`).toMatchObject({ text: "6 Sep", anchor: "start" });
      expect(ticks.at(-1), `${width}`).toMatchObject({ text: "31 Oct", anchor: "end" });
      for (let i = 1; i < ticks.length; i++) {
        expect(
          (ticks[i]?.x ?? 0) - (ticks[i - 1]?.x ?? 0),
          `${width}: ${ticks.map((t) => t.text)}`,
        ).toBeGreaterThanOrEqual(56);
      }
      expect(new Set(ticks.map((t) => t.text)).size).toBe(ticks.length);
    }
    // Long spans label months, ends included.
    const long = dateTicks(timeScale([new Date(Date.UTC(2025, 0, 9)), last], 360), 360);
    expect(long[0]?.text).toBe("Jan 2025");
    expect(long.at(-1)?.text).toBe("Oct 2026");
  });

  it("keeps the two-game minimum by default", async () => {
    const view = render(<AccuracyTrend points={one} rolling={[]} label="Accuracy" />);
    await act(async () => {});
    expect(view.container.textContent).toContain("Not enough games yet");
  });
});

describe("dark-mode weight", () => {
  it("steps the wins bar down to ink-soft at night, so it is not the heaviest mark", () => {
    const { container } = render(
      <ResultSplit rows={[{ key: "rapid", label: "Rapid", wins: 5, draws: 1, losses: 4 }]} />,
    );
    const wins = container.querySelector('[data-segment="wins"] [data-fill]');
    expect(wins?.getAttribute("class")).toContain("dark:bg-ink-soft");
  });
});
