import { KpiTrendGrid, type KpiTrendItem } from "@ci/ui/profile";
import { type TimelineSeries, WeaknessTimeline } from "@ci/ui/timeline";
import type { ReactNode } from "react";
import type { WeeklyReviewView } from "../../../../server/coaching";
import type { TimeClass } from "../../../../server/games/types";
import { type KpiTrend, kpiDef, type WeaknessDetail } from "../../../../server/profile";
import { PageHeading } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";
import {
  BeforeAfter,
  type BeforeAfterRow,
  beforeAfterRows,
  dayWords,
  isoDay,
} from "./before-after";
import { type FirstReview, WeeklyReviews } from "./weekly-reviews";

// The progress page (M5b Task 10, spec §10a.4): before and after first, then the rating and the
// KPIs on one shared time axis, the weakness timeline and the weekly reviews. Everything is built
// from the read models' objects; nothing here computes a rate or an interval.

/** The KPIs the page draws, rating first (§10a.4). */
export const PROGRESS_METRICS = [
  "rating",
  "accuracy",
  "accuracy.opening",
  "accuracy.middlegame",
  "accuracy.endgame",
  "blunders_per_game",
  "conversion_rate",
  "resilience_rate",
  "blunder_rate_low_clock",
  "missed_punish_rate",
] as const;

type NUnit = { one: string; many: string };
const GAMES: NUnit = { one: "game", many: "games" };

/**
 * The multiples grouped by what their `n` counts and the catalogue minimum, so each group's key
 * states its own threshold truthfully (KpiTrendGrid takes one `minN` per grid).
 */
const GROUPS: { id: string; title: string; metrics: string[]; nUnit: NUnit }[] = [
  {
    id: "accuracy",
    title: "Accuracy and blunders",
    metrics: [
      "accuracy",
      "accuracy.opening",
      "accuracy.middlegame",
      "accuracy.endgame",
      "blunders_per_game",
    ],
    nUnit: GAMES,
  },
  {
    id: "convert",
    title: "Converting and saving",
    metrics: ["conversion_rate", "resilience_rate"],
    nUnit: GAMES,
  },
  {
    id: "low-clock",
    title: "On a low clock",
    metrics: ["blunder_rate_low_clock"],
    nUnit: { one: "low-clock move", many: "low-clock moves" },
  },
  {
    id: "punish",
    title: "Punishing mistakes",
    metrics: ["missed_punish_rate"],
    nUnit: { one: "chance", many: "chances" },
  },
];

/** What `n` counts per item where it is narrower than its group's. */
const ITEM_UNIT: Record<string, NUnit> = {
  conversion_rate: { one: "game with a winning position", many: "games with a winning position" },
  resilience_rate: { one: "game with a lost position", many: "games with a lost position" },
};

const TIME_CLASS: Record<TimeClass, string> = {
  bullet: "bullet",
  blitz: "blitz",
  rapid: "rapid",
  classical: "classical",
  daily: "daily",
};

const DAY = 86_400_000;
const count = new Intl.NumberFormat("en-GB");

export type ProgressInput = {
  now: Date;
  basis: { analysedGames: number; timeClass: TimeClass } | null;
  trends: KpiTrend[];
  weaknesses: WeaknessDetail[];
  /** The newest report's date, drawn as an event on the KPI axis. */
  reportAt: Date | null;
  weekly: WeeklyReviewView[];
};

type Grid = { id: string; title: string; minN: number; nUnit: NUnit; items: KpiTrendItem[] };

export type ProgressViewModel = {
  basis: ProgressInput["basis"];
  beforeAfter: BeforeAfterRow[];
  kpis: {
    range: { from: string; to: string };
    events: { date: string; label: string }[];
    rating: Grid;
    groups: Grid[];
  } | null;
  timeline: { series: TimelineSeries[]; range: { from: string; to: string } } | null;
  weekly: { reviews: WeeklyReviewView[]; first: FirstReview };
};

/** "2026-W46" to that ISO week's Monday, "2026-11-09"; null when it is not a real ISO week. */
export function weekMonday(week: string): string | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(week);
  if (!m) return null;
  const year = Number(m[1]);
  const w = Number(m[2]);
  if (w < 1 || w > 53) return null;
  const jan4 = Date.UTC(year, 0, 4);
  const dow = (new Date(jan4).getUTCDay() + 6) % 7;
  const monday = jan4 - dow * DAY + (w - 1) * 7 * DAY;
  // The week's Thursday decides its ISO year (week 53 exists only in some years).
  if (new Date(monday + 3 * DAY).getUTCFullYear() !== year) return null;
  return isoDay(new Date(monday));
}

const addDays = (iso: string, days: number) =>
  isoDay(new Date(Date.parse(`${iso}T00:00:00Z`) + days * DAY));

function item(t: KpiTrend): KpiTrendItem {
  const points = t.points.flatMap((p) => {
    const week = weekMonday(p.week);
    return week ? [{ week, value: p.value, low: p.low, high: p.high, n: p.n }] : [];
  });
  const nUnit = ITEM_UNIT[t.metric];
  return { metric: t.metric, label: t.label, unit: t.unit, points, ...(nUnit ? { nUnit } : {}) };
}

function grid(
  id: string,
  title: string,
  metrics: string[],
  nUnit: NUnit,
  byMetric: Map<string, KpiTrendItem>,
): Grid {
  const items = metrics.map(
    (m) => byMetric.get(m) ?? { metric: m, label: kpiDef(m)?.label ?? m, unit: null, points: [] },
  );
  // The strictest catalogue minimum of the group, so no week is drawn as sufficient too early.
  const minN = Math.max(1, ...metrics.map((m) => kpiDef(m)?.minN ?? 1));
  return { id, title, minN, nUnit, items };
}

export function toProgressView(input: ProgressInput): ProgressViewModel {
  const byMetric = new Map(input.trends.map((t) => [t.metric, item(t)]));
  const weeks = [...byMetric.values()].flatMap((i) => i.points.map((p) => p.week)).sort();
  const first = weeks[0];
  const last = weeks.at(-1);

  let kpis: ProgressViewModel["kpis"] = null;
  if (first && last) {
    const reportDay = input.reportAt ? isoDay(input.reportAt) : null;
    const events =
      reportDay && reportDay >= first && reportDay <= addDays(last, 6)
        ? [{ date: reportDay, label: "your report was built" }]
        : [];
    kpis = {
      range: { from: first, to: last },
      events,
      rating: grid("rating", "Rating", ["rating"], GAMES, byMetric),
      groups: GROUPS.map((g) => grid(g.id, g.title, g.metrics, g.nUnit, byMetric)),
    };
  }

  const series = input.weaknesses.map((w) => w.timeline).filter((s) => s.segments.length > 0);
  const today = isoDay(input.now);
  const starts = series
    .flatMap((s) => [...s.segments.map((g) => g.from), ...(s.onset ? [s.onset.low] : [])])
    .concat(first ? [first] : [])
    .sort();
  const timeline =
    series.length > 0 && starts[0]
      ? { series, range: { from: [starts[0], today].sort()[0] ?? today, to: today } }
      : null;

  // The newest week with games; Rust reviews it on the Monday after (G2, `reviewed_week`).
  const played = input.trends
    .flatMap((t) => t.points.filter((p) => p.n > 0).map((p) => weekMonday(p.week)))
    .filter((w): w is string => w != null)
    .sort()
    .at(-1);
  const due = played ? addDays(played, 7) : null;
  const firstReview: FirstReview =
    input.weekly.length === 0 && played && due && due >= today ? { week: played, due } : null;

  return {
    basis: input.basis,
    beforeAfter: beforeAfterRows(input.weaknesses),
    kpis,
    timeline,
    weekly: { reviews: input.weekly, first: firstReview },
  };
}

function Section({
  id,
  title,
  lede,
  children,
}: {
  id: string;
  title: string;
  lede?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-5">
      <SectionHead id={id} title={title}>
        {lede ? <p className="m-0 max-w-2xl text-sm text-ink-soft">{lede}</p> : null}
      </SectionHead>
      {children}
    </section>
  );
}

/**
 * One key per page: the first KPI group shows KpiTrendGrid's key and report list; the rating and
 * the other groups hide theirs and say the one thing that differs (their threshold) in a line.
 */
const NO_KEY = "[&>div:first-child]:hidden";

function ReportNote({ events }: { events: { date: string; label: string }[] }) {
  const ev = events[0];
  if (!ev) return null;
  return (
    <>
      {" "}
      Dashed rule 1 is the day <time dateTime={ev.date}>{dayWords(ev.date)}</time> {ev.label}.
    </>
  );
}

const sheet = "rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:py-8";

export function ProgressView({
  view,
  scopeSwitch,
}: {
  view: ProgressViewModel;
  scopeSwitch?: ReactNode;
}) {
  const basis = view.basis
    ? `Based on ${count.format(view.basis.analysedGames)} analysed ${TIME_CLASS[view.basis.timeClass]} ${
        view.basis.analysedGames === 1 ? "game" : "games"
      }.`
    : null;
  return (
    <div className="flex flex-col gap-12">
      <div className="flex flex-col gap-5">
        <PageHeading
          title="Progress"
          lede={<>What has changed in your games, week by week.{basis ? ` ${basis}` : ""}</>}
        />
        {scopeSwitch}
      </div>

      <Section
        id="before-after-heading"
        title="Before and after"
        lede="Your rate either side of each change the detector has called."
      >
        <div data-surface="sheet" className={sheet}>
          <BeforeAfter rows={view.beforeAfter} />
        </div>
      </Section>

      {view.kpis ? (
        <>
          <Section
            id="rating-heading"
            title="Rating over time"
            lede="Your recorded rating; it moves for many reasons."
          >
            <div
              data-surface="sheet"
              className={`${sheet} grid grid-cols-1 gap-x-10 gap-y-4 lg:grid-cols-5 lg:items-end`}
            >
              <KpiTrendGrid
                items={view.kpis.rating.items}
                range={view.kpis.range}
                events={view.kpis.events}
                minN={view.kpis.rating.minN}
                nUnit={view.kpis.rating.nUnit}
                headingLevel={3}
                className={`lg:col-span-3 ${NO_KEY}`}
              />
              <p className="m-0 max-w-md text-sm text-ink-faint lg:col-span-2">
                The line joins weeks with a recorded rating; a break is a week without games.
                <ReportNote events={view.kpis.events} />
              </p>
            </div>
          </Section>

          <Section
            id="kpi-heading"
            title="How you play"
            lede="Each week's value with its stored 80% range, on the same weeks as your rating."
          >
            <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
              {view.kpis.groups.map((g, i) => (
                <section
                  key={g.id}
                  data-surface="sheet"
                  className={`${sheet} flex flex-col gap-4 ${i === 0 ? "lg:col-span-2" : i === 1 ? "lg:row-span-2" : ""}`}
                  aria-labelledby={`kpi-${g.id}`}
                >
                  <div className="flex flex-col gap-1">
                    <h3
                      id={`kpi-${g.id}`}
                      className="m-0 font-serif text-xl font-semibold text-ink"
                    >
                      {g.title}
                    </h3>
                    {i === 0 ? null : (
                      <p className="m-0 text-sm text-ink-faint">
                        Marks as in the key above; hollow weeks had fewer than {g.minN}{" "}
                        {g.nUnit.many}.
                      </p>
                    )}
                  </div>
                  <KpiTrendGrid
                    items={g.items}
                    range={view.kpis?.range ?? { from: "", to: "" }}
                    events={view.kpis?.events ?? []}
                    minN={g.minN}
                    nUnit={g.nUnit}
                    headingLevel={4}
                    className={i === 0 ? undefined : NO_KEY}
                  />
                </section>
              ))}
            </div>
          </Section>
        </>
      ) : (
        <Section id="rating-heading" title="Rating and KPIs">
          <p className="max-w-2xl text-base text-ink-soft">
            Weekly trends appear once a week of your games is analysed.
          </p>
        </Section>
      )}

      <Section
        id="timeline-heading"
        title="Weakness timeline"
        lede="Each measured weakness and the states the detector has called, with onset markers."
      >
        <div data-surface="sheet" className={sheet}>
          {view.timeline ? (
            <WeaknessTimeline series={view.timeline.series} range={view.timeline.range} />
          ) : (
            <p className="m-0 max-w-2xl text-base text-ink-soft">
              No weakness has a recorded state yet. Lanes appear once the detector calls one.
            </p>
          )}
        </div>
      </Section>

      <Section
        id="weekly-heading"
        title="Weekly reviews"
        lede="Your coach's notes on each week, checked against your games before they are shown."
      >
        <div data-surface="sheet" className={sheet}>
          <WeeklyReviews reviews={view.weekly.reviews} first={view.weekly.first} />
        </div>
      </Section>
    </div>
  );
}
