import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { timelineFixtures } from "../src/fixtures/index.js";
import {
  eventRows,
  shortSpan,
  stateAtEnd,
  stateStyle,
  type WeaknessState,
  WeaknessTimeline,
  type WeaknessTimelineProps,
} from "../src/timeline/index.js";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

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
  it("separates every state without colour: pattern, angle and weight are unique together", () => {
    const shapes = new Set(
      STATES.map(
        (s) => `${stateStyle[s].pattern}|${stateStyle[s].angle ?? ""}|${stateStyle[s].weight}`,
      ),
    );
    expect(shapes.size).toBe(6);
  });

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
  });

  it("makes the scroller a named tab stop only when it overflows", () => {
    const { container, unmount } = render(<WeaknessTimeline {...timelineFixtures} />);
    let scroller = container.querySelector("[data-timeline-scroll]") as HTMLElement;
    expect(scroller).not.toHaveAttribute("tabindex");
    expect(scroller).not.toHaveAttribute("role");
    unmount();
    vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(608);
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(296);
    const second = render(<WeaknessTimeline {...timelineFixtures} />);
    scroller = second.container.querySelector("[data-timeline-scroll]") as HTMLElement;
    expect(scroller).toHaveAttribute("tabindex", "0");
    expect(scroller).toHaveAccessibleName("Weakness timeline, scrolls sideways");
    // It opens on the latest months, and offers the way back.
    expect(screen.getByRole("button", { name: "Show earlier months" })).toBeInTheDocument();
  });

  it("names the figure", () => {
    render(<WeaknessTimeline {...timelineFixtures} label="Progress, weaknesses" />);
    expect(screen.getByRole("figure", { name: "Progress, weaknesses" })).toBeInTheDocument();
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

  it("is read once: the grid carries every state, with no duplicate summary", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(screen.queryByText(/not an issue from 1 Mar 2026/)).toBeNull();
    const rows = within(screen.getByRole("grid")).getAllByRole("row");
    const rook = rows[0] as HTMLElement;
    expect(
      within(rook)
        .getAllByRole("gridcell")
        .map((c) => c.getAttribute("aria-label")),
    ).toEqual([
      "Not an issue, 1 Mar 2026 to 10 Apr 2026, 1 of 6",
      "Emerging, 10 Apr 2026 to 5 May 2026, 2 of 6",
      "Active, 5 May 2026 to 1 Jul 2026, 3 of 6",
      "Improving, 1 Jul 2026 to 20 Aug 2026, 4 of 6",
      "Resolved, 20 Aug 2026 to 5 Oct 2026, 5 of 6",
      "Relapsed, 5 Oct 2026 to now, 6 of 6",
    ]);
    expect(within(rook).getByRole("rowheader")).toHaveTextContent(
      "Estimated onset 10 Apr 2026, between 28 Mar 2026 and 24 Apr 2026.",
    );
    // No sr-only text outside the grid repeats a lane.
    const outside = [...container.querySelectorAll(".sr-only")].filter(
      (el) => !el.closest('[role="grid"]'),
    );
    for (const el of outside) expect(el.textContent).not.toMatch(/Rook endgames/);
  });

  it("splits the readout at exactly 640px: @max is below 640, @min is 640 and up", async () => {
    const { compile } = await import("tailwindcss");
    const css = (await compile("@tailwind utilities;")).build([
      "@max-[640px]:hidden",
      "@min-[640px]:hidden",
    ]);
    expect(css).toContain("@container (width < 640px)");
    expect(css).toContain("@container (width >= 640px)");
  });

  it("marks plan and drill events with different shapes", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(container.querySelectorAll("[data-event='plan'] path")).toHaveLength(1);
    expect(container.querySelectorAll("[data-event='drill'] rect")).toHaveLength(1);
  });

  it("shows each lane's current state as visible text with a swatch", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(screen.getByText("Now relapsed")).toBeVisible();
    const now = container.querySelector("[data-now]");
    expect(now?.querySelector("svg")).not.toBeNull();
    expect(now?.className).toContain("text-ink-soft");
  });

  it("takes now from the segment covering range.to, whatever the input order", () => {
    const s = {
      id: "x",
      label: "X",
      segments: [
        { state: "resolved" as const, from: "2026-10-10", to: "2026-12-01" },
        { state: "active" as const, from: "2026-09-01", to: "2026-10-10" },
      ],
    };
    expect(stateAtEnd(s, range)).toBe("resolved");
    expect(stateAtEnd({ ...s, segments: [...s.segments].reverse() }, range)).toBe("resolved");
  });

  it("says No data yet for an empty series", () => {
    render(<WeaknessTimeline range={range} series={[{ id: "e", label: "Empty", segments: [] }]} />);
    expect(screen.getByText("No data yet")).toBeInTheDocument();
  });

  it("labels the range end", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(container.querySelector("[data-range-end]")?.textContent).toBe("1 Nov");
  });

  it("offsets colliding events onto a second row", () => {
    expect(eventRows([0.1, 0.11, 0.5])).toEqual([0, 1, 0]);
    const { container } = render(
      <WeaknessTimeline
        {...timelineFixtures}
        events={[
          { date: "2026-05-20", kind: "plan", label: "A" },
          { date: "2026-05-20", kind: "drill", label: "B" },
        ]}
      />,
    );
    const rows = [...container.querySelectorAll("[data-event]")].map((e) =>
      e.getAttribute("data-row"),
    );
    expect(rows).toEqual(["0", "1"]);
  });
});

describe("WeaknessTimeline keyboard", () => {
  const cells = (container: HTMLElement, lane: string) =>
    [...container.querySelectorAll(`[data-lane="${lane}"] [data-cell]`)] as HTMLElement[];
  const key = (k: string) => fireEvent.keyDown(document.activeElement as Element, { key: k });

  it("has one tab stop: the first lane's latest segment", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    const stops = container.querySelectorAll('[data-cell][tabindex="0"]');
    expect(stops).toHaveLength(1);
    expect(stops[0]).toBe(cells(container, "rook-endgames").at(-1));
  });

  it("moves with the arrow keys, Home and End, and shows the readout on focus", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    const rook = cells(container, "rook-endgames");
    const readout = container.querySelector("[data-readout]") as HTMLElement;
    expect(readout).toHaveTextContent("Hover, tap or use the arrow keys");

    act(() => rook.at(-1)?.focus());
    expect(readout).toHaveTextContent("Rook endgames: Relapsed, 5 Oct 2026 to now");
    key("ArrowLeft");
    expect(document.activeElement).toBe(rook[4]);
    expect(readout).toHaveTextContent("Resolved, 20 Aug 2026 to 5 Oct 2026");
    expect(rook[4]).toHaveAttribute("tabindex", "0");
    expect(rook.at(-1)).toHaveAttribute("tabindex", "-1");

    key("Home");
    expect(document.activeElement).toBe(rook[0]);
    key("End");
    expect(document.activeElement).toBe(rook.at(-1));

    // Down keeps the place in time: from 5 Oct the next lane is resolved.
    key("ArrowDown");
    const time = cells(container, "time-trouble");
    expect(document.activeElement).toBe(time.at(-1));
    // Up from "resolved since 1 Sep" lands on what rook endgames were mid-range: resolved.
    key("ArrowUp");
    expect(document.activeElement).toBe(rook[4]);
  });

  it("names every cell with its lane, state and dates", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    expect(cells(container, "rook-endgames")[2]).toHaveAccessibleName(
      "Active, 5 May 2026 to 1 Jul 2026, 3 of 6",
    );
  });

  it("shows the readout outside the scroller, also for the last lane on hover", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    const scroller = container.querySelector("[data-timeline-scroll]") as HTMLElement;
    const readout = container.querySelector("[data-readout]") as HTMLElement;
    expect(scroller.contains(readout)).toBe(false);
    const last = cells(container, "knight-forks").at(-1) as HTMLElement;
    fireEvent.pointerEnter(last);
    expect(readout).toHaveTextContent("Missed knight forks: Emerging, 1 Aug 2026 to now");
    fireEvent.pointerLeave(last);
    expect(readout).toHaveTextContent("Hover, tap or use the arrow keys");
  });

  it("is an ARIA grid: one row per lane, a row header, and gridcells that take the keys", () => {
    render(<WeaknessTimeline {...timelineFixtures} />);
    const grid = screen.getByRole("grid");
    const rows = within(grid).getAllByRole("row");
    expect(rows).toHaveLength(3);
    const first = rows[0] as HTMLElement;
    expect(within(first).getByRole("rowheader")).toHaveTextContent("Rook endgames");
    expect(within(first).getAllByRole("gridcell")).toHaveLength(6);
    expect(within(grid).queryByRole("img")).toBeNull();
  });

  it("keeps the roving cursor inside the data when the series change", () => {
    const { container, rerender } = render(<WeaknessTimeline {...timelineFixtures} />);
    const rook = cells(container, "rook-endgames");
    act(() => rook.at(-1)?.focus());
    rerender(
      <WeaknessTimeline
        {...timelineFixtures}
        series={[
          {
            id: "short",
            label: "Short",
            segments: [{ state: "active", from: "2026-03-01", to: null }],
          },
        ]}
      />,
    );
    const stops = container.querySelectorAll('[data-cell][tabindex="0"]');
    expect(stops).toHaveLength(1);
    expect(stops[0]).toBe(cells(container, "short")[0]);
  });
});

describe("WeaknessTimeline readout", () => {
  it("formats short spans for narrow screens", () => {
    expect(shortSpan({ state: "resolved", from: "2026-08-20", to: "2026-10-05" })).toBe(
      "20\u00a0Aug\u20135\u00a0Oct\u00a02026",
    );
    expect(shortSpan({ state: "active", from: "2025-12-20", to: "2026-01-05" })).toBe(
      "20\u00a0Dec\u00a02025\u20135\u00a0Jan\u00a02026",
    );
    expect(shortSpan({ state: "relapsed", from: "2026-10-05", to: null })).toBe(
      "since 5\u00a0Oct\u00a02026",
    );
  });

  it("wraps instead of truncating, with the short dates below 640px and the long ones above", () => {
    const { container } = render(<WeaknessTimeline {...timelineFixtures} />);
    const cell = container.querySelectorAll('[data-lane="rook-endgames"] [data-cell]')[4];
    fireEvent.pointerEnter(cell as Element);
    const readout = container.querySelector("[data-readout]") as HTMLElement;
    for (const el of [readout, ...readout.querySelectorAll("*")]) {
      expect(el.getAttribute("class") ?? "").not.toMatch(
        /truncate|line-clamp|text-ellipsis|(^|[\s:])h-\d/,
      );
    }
    const text = readout.querySelector("[data-readout-text]") as HTMLElement;
    expect(text.className).toContain("min-w-0");
    expect(text.className).toContain("flex-1");
    const short = readout.querySelector("[data-readout-short]") as HTMLElement;
    const long = readout.querySelector("[data-readout-long]") as HTMLElement;
    expect(short.textContent).toBe("Resolved, 20\u00a0Aug\u20135\u00a0Oct\u00a02026");
    expect(short.className).toContain("@min-[640px]:hidden");
    expect(long.textContent).toBe("Resolved, 20\u00a0Aug\u00a02026 to 5\u00a0Oct\u00a02026");
    expect(long.className).toContain("@max-[640px]:hidden");
  });
});
