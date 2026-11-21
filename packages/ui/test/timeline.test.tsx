import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { timelineFixtures } from "../src/fixtures/index.js";
import {
  stateStyle,
  type WeaknessState,
  WeaknessTimeline,
  type WeaknessTimelineProps,
} from "../src/timeline/index.js";

afterEach(cleanup);

const pct = (el: Element | null, attr: string) => {
  const v = el?.getAttribute(attr);
  if (!v?.endsWith("%")) throw new Error(`${attr} is not a percentage: ${v}`);
  return Number.parseFloat(v);
};

type Series = WeaknessTimelineProps["series"][number];
const range = { from: "2026-09-01", to: "2026-11-01" };
const oneSeries = (
  segments: Series["segments"],
  onset?: Series["onset"],
): WeaknessTimelineProps => ({
  range,
  series: [{ id: "s", label: "Back-rank threats", segments, ...(onset ? { onset } : {}) }],
});

const STATES: WeaknessState[] = [
  "not_an_issue",
  "emerging",
  "active",
  "improving",
  "resolved",
  "relapsed",
];
const LABELS = ["Not an issue", "Emerging", "Active", "Improving", "Resolved", "Relapsed"];

describe("stateStyle", () => {
  it("labels the six states, and no two share both colour and pattern", () => {
    expect(STATES.map((s) => stateStyle[s].label)).toEqual(LABELS);
    expect(new Set(STATES.map((s) => stateStyle[s].pattern))).toEqual(
      new Set(["none", "hatch", "dots"]),
    );
    const combos = new Set(STATES.map((s) => `${stateStyle[s].fill}|${stateStyle[s].pattern}`));
    expect(combos.size).toBe(6);
  });
});

describe("WeaknessTimeline", () => {
  it("lists the six state labels in its legend", () => {
    render(<WeaknessTimeline {...timelineFixtures} />);
    const legend = screen.getByRole("list", { name: "Key" });
    for (const label of LABELS) expect(within(legend).getByText(label)).toBeInTheDocument();
  });

  it("sizes a one-month segment in a two-month range at 50% of the lane", () => {
    const { container } = render(
      <WeaknessTimeline
        {...oneSeries([
          { state: "active", from: "2026-09-01", to: "2026-10-01" },
          { state: "improving", from: "2026-10-01", to: "2026-11-01" },
        ])}
      />,
    );
    const seg = container.querySelector("[data-segment='active']");
    expect(pct(seg, "x")).toBeCloseTo(0, 1);
    expect(Math.abs(pct(seg, "width") - 50)).toBeLessThanOrEqual(0.5);
  });

  it("extends an open segment (to: null) to range.to", () => {
    const { container } = render(
      <WeaknessTimeline {...oneSeries([{ state: "emerging", from: "2026-10-01", to: null }])} />,
    );
    const seg = container.querySelector("[data-segment='emerging']");
    expect(pct(seg, "x") + pct(seg, "width")).toBeCloseTo(100, 1);
  });

  it("draws the onset as a pen tick with a whisker spanning low to high", () => {
    // A calendar axis: September and October are half the range each, days spread in their month.
    const { container } = render(
      <WeaknessTimeline
        {...oneSeries([{ state: "emerging", from: "2026-09-16", to: null }], {
          date: "2026-09-16",
          low: "2026-09-08",
          high: "2026-09-24",
        })}
      />,
    );
    const whisker = container.querySelector("[data-onset-whisker]");
    expect(pct(whisker, "x1")).toBeCloseTo((7 / 30 / 2) * 100, 1);
    expect(pct(whisker, "x2")).toBeCloseTo((23 / 30 / 2) * 100, 1);
    const tick = container.querySelector("[data-onset-tick]");
    expect(pct(tick, "x1")).toBeCloseTo((15 / 30 / 2) * 100, 1);
    expect(tick?.getAttribute("class")).toContain("stroke-pen");
  });

  it("timeline scrolls inside its container", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    const scroller = container.querySelector("[data-timeline-scroll]") as HTMLElement;
    expect(getComputedStyle(scroller).overflowX).toBe("auto");
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper).not.toBe(scroller);
    expect(getComputedStyle(wrapper).overflowX).not.toBe("auto");
    // A scrolling region is reachable from the keyboard and named.
    expect(scroller).toHaveAttribute("tabindex", "0");
    expect(scroller).toHaveAccessibleName();
  });

  it("renders one lane per series: 12 series, 12 lanes", () => {
    const series = Array.from({ length: 12 }, (_, i) => ({
      id: `w${i + 1}`,
      label: `Weakness ${i + 1}`,
      segments: [{ state: "active" as const, from: "2026-09-01", to: null }],
    }));
    const { container } = render(<WeaknessTimeline range={range} series={series} />);
    expect(container.querySelectorAll("[data-lane]")).toHaveLength(12);
  });

  it("spells out each lane's states for assistive tech", () => {
    render(<WeaknessTimeline {...timelineFixtures} />);
    expect(
      screen.getByText(/Rook endgames: not an issue from 1 Mar 2026; emerging from 10 Apr 2026/),
    ).toBeInTheDocument();
    expect(screen.getByText(/relapsed from 5 Oct 2026 to now/)).toBeInTheDocument();
  });

  it("marks plan and drill events with different shapes", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(container.querySelectorAll("[data-event='plan'] path")).toHaveLength(1);
    expect(container.querySelectorAll("[data-event='drill'] rect")).toHaveLength(1);
  });
});
