import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import {
  monthlyRatings,
  RatingTrajectory,
  type RatingTrajectoryProps,
  ratingDomain,
  windowLabels,
} from "../src/charts/index.js";

// The rating trajectory (F6): one ink line of the pre-game rating, the peak marked and labelled,
// the current rating at the end, and the two compared windows shaded and named. A single series,
// so no legend; a table view of the months; a crosshair readout on hover. Synthetic data (tests).

class FixedResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element) {
    const rect = { width: 640, height: 0, top: 0, left: 0, right: 640, bottom: 0, x: 0, y: 0 };
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
afterEach(() => cleanup());

const DAY = 86_400_000;
const START = Date.UTC(2026, 0, 1);
const iso = (i: number) => new Date(START + i * DAY).toISOString();

function props(): RatingTrajectoryProps {
  const ratings = Array.from({ length: 90 }, (_, i) =>
    i <= 40 ? 800 + i * 7 : 1080 - (i - 40) * 5,
  );
  return {
    label: "Rapid rating over time",
    points: ratings.map((rating, i) => ({ id: `g${i}`, date: iso(i), rating })),
    peak: { date: iso(40), rating: 1080, label: "Peak 1,080, February 2026" },
    current: { date: iso(90), rating: 830, label: "Now 830" },
    windows: [
      { from: iso(25), to: iso(54), label: "Around your peak" },
      { from: iso(60), to: iso(89), label: "Your last 30 games" },
    ],
  };
}

const nextFrame = () =>
  act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  });

describe("ratingDomain", () => {
  it("rounds out to a step that gives at most five gridlines", () => {
    expect(ratingDomain([816, 1097])).toEqual({ lo: 800, hi: 1100, ticks: [800, 900, 1000, 1100] });
    expect(ratingDomain([1490, 1510])).toEqual({ lo: 1475, hi: 1525, ticks: [1475, 1500, 1525] });
  });

  it("widens a single value by one step each way", () => {
    expect(ratingDomain([900])).toEqual({ lo: 875, hi: 925, ticks: [875, 900, 925] });
  });
});

describe("windowLabels", () => {
  const spans = (a: [number, number], b: [number, number]) => [
    { x0: a[0], x1: a[1], label: "Around your peak", short: "Then" },
    { x0: b[0], x1: b[1], label: "Your last 30 games", short: "Now" },
  ];

  it("centres each label over its span when they fit", () => {
    const out = windowLabels(spans([150, 250], [400, 500]), 640);
    expect(out.map((l) => [l.text, l.x, l.anchor])).toEqual([
      ["Around your peak", 200, "middle"],
      ["Your last 30 games", 450, "middle"],
    ]);
  });

  it("shortens labels that would collide, and pushes them apart", () => {
    const out = windowLabels(spans([580, 600], [601, 625]), 640);
    expect(out.map((l) => l.text)).toEqual(["Then", "Now"]);
    const [a, b] = out;
    expect(a?.anchor).toBe("end");
    expect(b?.anchor).toBe("start");
    expect((a?.x ?? 0) <= (b?.x ?? 0)).toBe(true);
  });
});

describe("monthlyRatings", () => {
  it("gives each UTC month its games, its last rating and its highest", () => {
    const rows = monthlyRatings([
      { id: "a", date: "2026-01-05T10:00:00Z", rating: 900 },
      { id: "b", date: "2026-01-20T10:00:00Z", rating: 950 },
      { id: "c", date: "2026-01-28T10:00:00Z", rating: 930 },
      { id: "d", date: "2026-02-02T10:00:00Z", rating: 910 },
    ]);
    expect(rows).toEqual([
      { month: "2026-01", label: "Jan 2026", games: 3, last: 930, high: 950 },
      { month: "2026-02", label: "Feb 2026", games: 1, last: 910, high: 910 },
    ]);
  });
});

describe("RatingTrajectory", () => {
  it("draws one labelled line with the peak and the current rating marked", async () => {
    const { container } = render(<RatingTrajectory {...props()} />);
    await nextFrame();
    const svg = screen.getByRole("img", { name: "Rapid rating over time" });
    expect(svg.querySelectorAll('[data-part="line"]')).toHaveLength(1);
    expect(container.querySelector('[data-part="peak"]')).not.toBeNull();
    expect(svg).toHaveTextContent("Peak 1,080, February 2026");
    expect(svg).toHaveTextContent("Now 830");
    expect(container.textContent).not.toMatch(/NaN|undefined|Infinity/);
  });

  it("shades and names the two compared windows", async () => {
    const { container } = render(<RatingTrajectory {...props()} />);
    await nextFrame();
    const windows = container.querySelectorAll('[data-part="window"]');
    expect(windows).toHaveLength(2);
    expect(container.querySelector("svg")).toHaveTextContent("Around your peak");
    expect(container.querySelector("svg")).toHaveTextContent("Your last 30 games");
  });

  it("reads out the nearest game under the pointer", async () => {
    const { container } = render(<RatingTrajectory {...props()} />);
    await nextFrame();
    const hit = container.querySelector('[data-part="hit"]') as HTMLElement;
    hit.getBoundingClientRect = () => ({ left: 0, top: 0, width: 640, height: 208 }) as DOMRect;
    fireEvent.pointerMove(hit, { clientX: 639, clientY: 100, pointerType: "mouse" });
    expect(container.querySelector('[data-part="readout"]')).toHaveTextContent("31 Mar 2026: 835");
    fireEvent.pointerLeave(hit);
    expect(container.querySelector('[data-part="readout"]')).toBeNull();
  });

  it("offers the months as a table", async () => {
    render(<RatingTrajectory {...props()} />);
    await nextFrame();
    const table = screen.getByRole("table", { name: "Rapid rating over time, by month" });
    expect(table.querySelectorAll("tbody tr")).toHaveLength(3);
  });

  it("paints gridlines, never a blank box, before it has measured", () => {
    const html = renderToString(<RatingTrajectory {...props()} />);
    expect(html).toContain('data-part="trajectory-placeholder"');
    expect(html).not.toMatch(/NaN|undefined/);
  });
});
