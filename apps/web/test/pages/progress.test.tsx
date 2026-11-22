// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import {
  PROGRESS_METRICS,
  type ProgressInput,
  ProgressView,
  toProgressView,
  weekMonday,
} from "../../src/app/(app)/app/progress/progress-view.js";
import type { WeeklyReviewView } from "../../src/server/coaching/index.js";
import type { KpiTrend, WeaknessDetail } from "../../src/server/profile/index.js";
import { DISCLOSURE, WEEKLY_CONTENT } from "../helpers/coaching.js";

// The progress page (M5b Task 10) from synthetic read-model objects (B15: no owner data). The
// loader's read-model test is progress-load.test.ts (node, so migrations run outside jsdom).

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

const BAD = /NaN|undefined|Infinity|null|\{\{/;
const NOW = new Date("2026-11-22T10:00:00Z");

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((q: string) => ({
    matches: false,
    media: q,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
});

const def = (metric: string) =>
  ({
    rating: { label: "Rating", unit: "elo" },
    accuracy: { label: "Accuracy", unit: "pct" },
    "accuracy.opening": { label: "Opening accuracy", unit: "pct" },
    "accuracy.middlegame": { label: "Middlegame accuracy", unit: "pct" },
    "accuracy.endgame": { label: "Endgame accuracy", unit: "pct" },
    blunders_per_game: { label: "Blunders per game", unit: "per_game" },
    blunder_rate_low_clock: { label: "Blunder rate on a low clock", unit: "rate" },
    conversion_rate: { label: "Winning positions converted", unit: "rate" },
    resilience_rate: { label: "Lost positions saved", unit: "rate" },
    missed_punish_rate: { label: "Mistakes left unpunished", unit: "rate" },
  })[metric] ?? { label: metric, unit: null };

function trend(metric: string, weeks: string[], value: (i: number) => number | null): KpiTrend {
  const d = def(metric);
  return {
    metric,
    label: d.label,
    unit: d.unit,
    current: null,
    points: weeks.map((week, i) => ({ week, value: value(i), low: null, high: null, n: 50 })),
  };
}

const WEEKS = ["2026-W40", "2026-W41", "2026-W42", "2026-W43", "2026-W44", "2026-W45"];

type HistoryRow = WeaknessDetail["history"][number];
const row = (over: Partial<HistoryRow>): HistoryRow => ({
  kind: "transition",
  fromState: null,
  toState: "active",
  at: null,
  onsetAt: null,
  onsetLow: null,
  onsetHigh: null,
  pChange: null,
  rateBefore: null,
  rateAfter: null,
  causeHints: [],
  counts: null,
  ...over,
});

function weakness(over: Partial<WeaknessDetail> = {}): WeaknessDetail {
  return {
    seriesKey: "W1",
    contractId: "W1",
    label: "Hung material",
    area: "tactics",
    state: "improving",
    rate: { mean: 0.03, low: 0.025, high: 0.036, level: 0.8 },
    evidence: { events: 38, chances: 1200, minEvidence: 1000, sufficient: true, needMore: null },
    band: { median: 0.02, p40: 0.018 },
    priority: 1,
    confidence: 0.9,
    pImp: 0.8,
    updatedAt: NOW,
    onset: null,
    // Shaped like Rust's rows: the confirming game writes a transition (no onset fields) and an
    // onset row with the same game, rates and counts, whose onset is the change point.
    history: [
      row({ toState: "active", at: new Date("2026-09-01T12:00:00Z") }),
      row({
        fromState: "active",
        toState: "improving",
        at: new Date("2026-10-21T12:00:00Z"),
        pChange: 0.93,
        rateBefore: 0.041,
        rateAfter: 0.026,
        counts: { events: 12, chances: 460 },
      }),
      row({
        kind: "onset",
        fromState: "active",
        toState: "improving",
        at: new Date("2026-10-21T12:00:00Z"),
        onsetAt: new Date("2026-10-14T12:00:00Z"),
        onsetLow: new Date("2026-10-08T12:00:00Z"),
        onsetHigh: new Date("2026-10-19T12:00:00Z"),
        pChange: 0.93,
        rateBefore: 0.041,
        rateAfter: 0.026,
        counts: { events: 12, chances: 460 },
      }),
    ],
    timeline: {
      id: "W1",
      label: "Hung material",
      segments: [
        { state: "active", from: "2026-09-01", to: "2026-10-21" },
        { state: "improving", from: "2026-10-21", to: null },
      ],
    },
    weekly: [],
    exemplars: null,
    ...over,
  };
}

function review(over: Partial<WeeklyReviewView> = {}): WeeklyReviewView {
  return {
    id: "r1",
    weekStart: "2026-11-09",
    status: "ready",
    content: WEEKLY_CONTENT,
    droppedCount: 0,
    aiDisclosure: { ...DISCLOSURE, promptVersion: "weekly@1" },
    ...over,
  };
}

function input(over: Partial<ProgressInput> = {}): ProgressInput {
  return {
    now: NOW,
    basis: { analysedGames: 214, timeClass: "rapid" },
    trends: PROGRESS_METRICS.map((m, i) =>
      trend(m, WEEKS.slice(i % 2), (j) => (m === "rating" ? 1500 + j * 10 : 50 + j)),
    ),
    weaknesses: [weakness()],
    reportAt: new Date("2026-10-20T08:00:00Z"),
    weekly: [review()],
    ...over,
  };
}

const show = (i: ProgressInput) => render(<ProgressView view={toProgressView(i)} />);
const section = (container: HTMLElement, id: string) =>
  container.querySelector(`#${id}`)?.closest("section") as HTMLElement;

describe("progress page", () => {
  it("before and after comes from the transition row", () => {
    const { container } = show(input());
    const block = section(container, "before-after-heading");
    // The date is the change point (the same-game onset row), not the confirming game.
    expect(block.textContent).toContain(
      "Hung material: 4.1% before, 2.6% after 14 Oct 2026, detected improving",
    );
    expect(block.querySelector('time[datetime="2026-10-14"]')).not.toBeNull();
    expect(block.textContent).not.toContain("21 Oct 2026");
    // One row per weakness: the onset row pairs with its transition, the first row has no rates.
    expect(block.querySelectorAll("li[data-change]")).toHaveLength(1);
    expect(block.textContent).not.toMatch(/fixed|thanks to|because|training/i);
  });

  it("without an onset row the date is the confirming game, said as such", () => {
    const w = weakness();
    const history = w.history.filter((h) => h.kind === "transition");
    const { container } = show(input({ weaknesses: [{ ...w, history }] }));
    const block = section(container, "before-after-heading");
    expect(block.textContent).toContain(
      "Hung material: 4.1% before, 2.6% after, detected improving on 21 Oct 2026",
    );
    expect(block.textContent).not.toContain("after 21 Oct");
  });

  it("each side of a before and after states its own n", () => {
    const { container } = show(input());
    const block = section(container, "before-after-heading");
    expect(block.textContent).toContain("After: 12 of 460 chances since 14 Oct 2026");
    expect(block.textContent).toContain("Before: from the earlier games of that stretch");
    // Never the current estimate's six-month n.
    expect(block.textContent).not.toMatch(/1,200|in all/);
  });

  it("before and after labels both values on the row's own scale", () => {
    const { container } = show(input());
    const block = section(container, "before-after-heading");
    const svg = block.querySelector("svg[data-dumbbell]");
    expect(svg?.textContent).toContain("4.1%");
    expect(svg?.textContent).toContain("2.6%");
    expect(block.textContent).toContain("Each row on its own scale from 0");
  });

  it("no transition gives the honest empty line", () => {
    const quiet = weakness({
      history: [{ ...(weakness().history[0] as WeaknessDetail["history"][number]) }],
    });
    const { container } = show(input({ weaknesses: [quiet] }));
    expect(section(container, "before-after-heading").textContent).toContain(
      "No change detected yet. Changes need enough games on both sides to be called.",
    );
  });

  it("a transition without both rates is not a before and after", () => {
    const w = weakness();
    const rows = w.history.map((h) => ({ ...h, rateAfter: null }));
    const { container } = show(input({ weaknesses: [{ ...w, history: rows }] }));
    expect(section(container, "before-after-heading").textContent).toContain(
      "No change detected yet.",
    );
  });

  it("rating is labelled without causal claim", () => {
    const { container } = show(input());
    const block = section(container, "rating-heading");
    expect(block.textContent).toContain("Your recorded rating; it moves for many reasons");
    expect(block.textContent).not.toMatch(/because|thanks to|improved by|training/i);
    expect(block.querySelector('[data-kpi="rating"]')).not.toBeNull();
  });

  it("kpi grid shares one range", () => {
    const view = toProgressView(input());
    // The scope's first to last KPI week, as Mondays.
    expect(view.kpis?.range).toEqual({ from: "2026-09-28", to: "2026-11-02" });
    const { container } = render(<ProgressView view={view} />);
    const figures = [...container.querySelectorAll("figure[data-kpi]")];
    expect(figures.map((f) => f.getAttribute("data-kpi")).sort()).toEqual(
      [...PROGRESS_METRICS].sort(),
    );
    const ends = new Set(
      figures.map((f) => {
        const spans = [...f.querySelectorAll("span")].map((s) => s.textContent ?? "");
        return spans.filter((t) => /\d{4}$|^\d+ \w{3}$/.test(t)).join("|");
      }),
    );
    expect(ends.size).toBe(1);
    // The report date is an event on that axis.
    expect(container.textContent).toContain("20 Oct 2026");
  });

  it("an empty metric reads as no games, never a zero", () => {
    const trends = input().trends.map((t) =>
      t.metric === "conversion_rate" ? { ...t, points: [] } : t,
    );
    const { container } = show(input({ trends }));
    expect(container.querySelector('figure[data-kpi="conversion_rate"]')).toBeNull();
    const early = container.querySelector('[data-early="conversion_rate"]') as HTMLElement;
    expect(early.textContent).toMatch(/No games with a winning position yet/);
    expect(early.textContent).not.toMatch(/\b0(\.0)?%/);
  });

  it("a metric with no week at its minimum is an early read, not a chart", () => {
    const trends = input().trends.map((t) =>
      t.metric === "blunders_per_game"
        ? { ...t, points: t.points.map((p) => ({ ...p, n: 6, value: 1.6 })) }
        : t,
    );
    const { container } = show(input({ trends }));
    expect(container.querySelector('figure[data-kpi="blunders_per_game"]')).toBeNull();
    const early = container.querySelector('[data-early="blunders_per_game"]') as HTMLElement;
    expect(early.textContent).toContain("Early read: 1.60 from 6 games in the week of 2 Nov 2026.");
    expect(early.textContent).toContain("A week counts from 20 games.");
    // Settled metrics keep their charts.
    expect(container.querySelector('figure[data-kpi="accuracy"]')).not.toBeNull();
  });

  it("fewer than three weeks collapse into one note and early reads", () => {
    const two = ["2026-W46", "2026-W47"];
    const { container } = show(
      input({
        trends: PROGRESS_METRICS.map((m) => trend(m, two, () => (m === "rating" ? 1480 : 40))),
      }),
    );
    expect(container.querySelectorAll("figure[data-kpi]")).toHaveLength(0);
    expect(container.textContent).toContain(
      "Trends appear after 3 weeks of games. You have 2 so far.",
    );
    expect(container.querySelectorAll("[data-early]")).toHaveLength(PROGRESS_METRICS.length);
    expect(container.querySelector('[data-early="rating"]')?.textContent).toContain("1,480");
  });

  it("the weakness timeline has a lane per measured weakness", () => {
    const other = weakness({
      seriesKey: "W2",
      label: "Missed opponent threat",
      timeline: {
        id: "W2",
        label: "Missed opponent threat",
        segments: [{ state: "emerging", from: "2026-10-01", to: null }],
        onset: { date: "2026-09-28", low: "2026-09-20", high: "2026-10-03" },
      },
    });
    const { container } = show(input({ weaknesses: [weakness(), other] }));
    const block = section(container, "timeline-heading");
    expect(block.textContent).toContain("Hung material");
    expect(block.textContent).toContain("Missed opponent threat");
  });

  it("a ready weekly review shows its parts with the disclosure", () => {
    const { container } = show(input());
    const block = section(container, "weekly-heading");
    expect(block.textContent).toContain("Week of 9 Nov 2026");
    expect(block.textContent).toContain("A steadier week.");
    expect(block.textContent).toContain("You took more time on critical moves.");
    expect(block.textContent).toContain("Hung pieces");
    expect(block.textContent).toContain("Fewer hung pieces.");
    expect(block.textContent).toContain("Keep checking captures before you move.");
    expect(block.querySelector("[data-ai-disclosure]")).not.toBeNull();
    expect(block.textContent).toContain("Written by AI");
  });

  it("a ready review without a stored disclosure shows no model text", () => {
    const { container } = show(input({ weekly: [review({ aiDisclosure: null })] }));
    const block = section(container, "weekly-heading");
    expect(block.textContent).not.toContain("A steadier week.");
    expect(block.textContent).toContain("couldn't be checked against your games");
  });

  it("pending weekly review shows the writing state", () => {
    const { container } = show(
      input({ weekly: [review({ status: "pending", content: null, aiDisclosure: null })] }),
    );
    expect(section(container, "weekly-heading").textContent).toContain(
      "Your weekly review is being written",
    );
  });

  it("dropped or failed reviews say why they are not shown", () => {
    for (const status of ["dropped", "failed"] as const) {
      const { container, unmount } = show(
        input({ weekly: [review({ status, content: null, aiDisclosure: null })] }),
      );
      expect(section(container, "weekly-heading").textContent).toContain(
        "This week's review couldn't be checked against your games, so it isn't shown.",
      );
      unmount();
    }
  });

  it("empty weekly reviews say when they appear", () => {
    const { container } = show(input({ weekly: [] }));
    const text = section(container, "weekly-heading").textContent ?? "";
    expect(text).toContain("Weekly reviews appear here once your coach writes them.");
    // The latest week with games (W45, 2 Nov) was due on 9 Nov, already past: no promise.
    expect(text).not.toMatch(/scheduled/);
  });

  it("empty weekly reviews give the first review's date when one is due", () => {
    const trends = input().trends.map((t) => ({
      ...t,
      points: [...t.points, { week: "2026-W47", value: 50, low: null, high: null, n: 3 }],
    }));
    const { container } = show(input({ weekly: [], trends }));
    const block = section(container, "weekly-heading");
    expect(block.textContent).toContain(
      "Your first, for the week of 16 Nov 2026, is scheduled for 23 Nov 2026.",
    );
    expect(block.querySelector('time[datetime="2026-11-23"]')).not.toBeNull();
  });

  it("the first review is the earliest week still due", () => {
    // Monday 23 Nov before the run: the week of 16 Nov is due today, so it is the first, even
    // though the week of 23 Nov already has games.
    const now = new Date("2026-11-23T02:00:00Z");
    const trends = input().trends.map((t) => ({
      ...t,
      points: [
        { week: "2026-W47", value: 50, low: null, high: null, n: 3 },
        { week: "2026-W48", value: 50, low: null, high: null, n: 1 },
      ],
    }));
    const { container } = show(input({ now, weekly: [], trends }));
    expect(section(container, "weekly-heading").textContent).toContain(
      "Your first, for the week of 16 Nov 2026, is scheduled for 23 Nov 2026.",
    );
  });

  it("a new account with two weeks renders without bad text", () => {
    const two = ["2026-W46", "2026-W47"];
    const { container } = show({
      now: NOW,
      basis: { analysedGames: 9, timeClass: "blitz" },
      trends: PROGRESS_METRICS.map((m) => trend(m, two, (i) => (i === 0 ? null : 40))),
      weaknesses: [],
      reportAt: null,
      weekly: [],
    });
    expect(container.innerHTML).not.toMatch(BAD);
    expect(container.textContent).toContain("No change detected yet.");
  });

  it("the rich page renders without bad text", () => {
    const { container } = show(input());
    expect(container.innerHTML).not.toMatch(BAD);
    for (const h of [
      "Progress",
      "Before and after",
      "Rating over time",
      "Rating",
      "Weakness timeline",
      "Weekly reviews",
    ]) {
      expect(screen.getAllByRole("heading", { name: h }).length).toBeGreaterThan(0);
    }
    within(container).getByText(/Based on/);
  });

  it("weekMonday maps an ISO week to its Monday", () => {
    expect(weekMonday("2026-W46")).toBe("2026-11-09");
    expect(weekMonday("2026-W01")).toBe("2025-12-29");
    expect(weekMonday("2020-W53")).toBe("2020-12-28");
    expect(weekMonday("nonsense")).toBeNull();
  });
});
