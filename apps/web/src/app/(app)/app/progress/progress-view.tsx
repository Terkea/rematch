import {
  dayText,
  type ExplainEntry,
  ExplainPopover,
  formatKpiValue,
  KpiTrendGrid,
  type KpiTrendItem,
  nText,
  onLine,
} from "@ci/ui/profile";
import { type TimelineSeries, WeaknessTimeline } from "@ci/ui/timeline";
import type { ReactNode } from "react";
import { weekMonday } from "../../../../lib/iso-week";
import type { WeeklyReviewView } from "../../../../server/coaching";
import type { TimeClass } from "../../../../server/games/types";
import { type KpiTrend, kpiDef, type WeaknessDetail } from "../../../../server/profile";
import { KPI_GLOSSARY } from "../../../../server/profile/glossary";
import { PageHeading } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";
import { BeforeAfter, type BeforeAfterRow, beforeAfterRows, isoDay } from "./before-after";
import { type FirstReview, WeeklyReviews } from "./weekly-reviews";

export { weekMonday } from "../../../../lib/iso-week";

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

/** Fewer weeks than this and no chart is drawn: one note and the early reads instead. */
export const MIN_TREND_WEEKS = 3;

type NUnit = { one: string; many: string };
const GAMES: NUnit = { one: "game", many: "games" };

/**
 * The multiples grouped by what their `n` counts and the catalogue minimum, so each group's key
 * states its own threshold truthfully (KpiTrendGrid takes one `minN` per grid). `caption` names
 * what the threshold counts when the items count narrower things than the group's unit.
 */
const GROUPS: { id: string; title: string; metrics: string[]; nUnit: NUnit; caption?: string }[] = [
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
    caption: "games with a winning or lost position",
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
  /** The filter bar clipped the trends to a date range (T3). */
  clipped?: boolean;
};

/** A metric without a trend to draw: its latest week in words, at body weight, never a headline. */
export type EarlyRead = {
  metric: string;
  label: string;
  text: string;
  explain?: ExplainEntry | null;
};

type Grid = {
  id: string;
  title: string;
  minN: number;
  nUnit: NUnit;
  caption: string;
  /** Metrics with at least one week at the minimum: drawn as multiples. */
  items: KpiTrendItem[];
  /** Metrics with none: early reads. */
  early: EarlyRead[];
};

export type ProgressViewModel = {
  basis: ProgressInput["basis"];
  clipped: boolean;
  beforeAfter: BeforeAfterRow[];
  kpis: {
    range: { from: string; to: string };
    events: { date: string; label: string }[];
    /** Distinct weeks in the range; below `MIN_TREND_WEEKS` nothing is charted. */
    weeks: number;
    collapsed: boolean;
    rating: Grid;
    groups: Grid[];
    /** The group that shows the grid's key (the first with a chart); the others hide theirs. */
    keyGroup: string | null;
  } | null;
  timeline: { series: TimelineSeries[]; range: { from: string; to: string } } | null;
  weekly: { reviews: WeeklyReviewView[]; first: FirstReview };
};

const addDays = (iso: string, days: number) =>
  isoDay(new Date(Date.parse(`${iso}T00:00:00Z`) + days * DAY));

function item(t: KpiTrend): KpiTrendItem {
  const points = t.points.flatMap((p) => {
    const week = weekMonday(p.week);
    return week ? [{ week, value: p.value, low: p.low, high: p.high, n: p.n }] : [];
  });
  const nUnit = ITEM_UNIT[t.metric];
  return {
    metric: t.metric,
    label: t.label,
    unit: t.unit,
    points,
    explain: KPI_GLOSSARY[t.metric] ?? null,
    ...(nUnit ? { nUnit } : {}),
  };
}

/** The latest measured week in words, with its n and, below the minimum, the threshold. */
function earlyRead(it: KpiTrendItem, minN: number, groupUnit: NUnit): EarlyRead {
  const unit = it.nUnit ?? groupUnit;
  const last = [...it.points]
    .reverse()
    .find((p) => p.value !== null && Number.isFinite(p.value) && p.n > 0);
  if (!last || last.value === null) {
    return {
      metric: it.metric,
      label: it.label,
      text: `No ${unit.many} yet.`,
      explain: it.explain,
    };
  }
  const value = formatKpiValue(last.value, it.unit);
  const when = `from ${nText(last.n, unit)} in the week of ${dayText(last.week)}.`;
  const text =
    last.n < minN
      ? `Early read: ${value} ${when} A week counts from ${count.format(minN)} ${minN === 1 ? unit.one : unit.many}.`
      : `Latest: ${value} ${when}`;
  return { metric: it.metric, label: it.label, text, explain: it.explain };
}

function grid(
  g: { id: string; title: string; metrics: string[]; nUnit: NUnit; caption?: string },
  byMetric: Map<string, KpiTrendItem>,
  collapsed: boolean,
): Grid {
  const all = g.metrics.map(
    (m) => byMetric.get(m) ?? { metric: m, label: kpiDef(m)?.label ?? m, unit: null, points: [] },
  );
  // The strictest catalogue minimum of the group, so no week is drawn as sufficient too early.
  const minN = Math.max(1, ...g.metrics.map((m) => kpiDef(m)?.minN ?? 1));
  const charted = (it: KpiTrendItem) => !collapsed && it.points.some((p) => onLine(p, minN));
  return {
    id: g.id,
    title: g.title,
    minN,
    nUnit: g.nUnit,
    caption: g.caption ?? g.nUnit.many,
    items: all.filter(charted),
    early: all.filter((it) => !charted(it)).map((it) => earlyRead(it, minN, g.nUnit)),
  };
}

export function toProgressView(input: ProgressInput): ProgressViewModel {
  const byMetric = new Map(input.trends.map((t) => [t.metric, item(t)]));
  const weeks = [
    ...new Set([...byMetric.values()].flatMap((i) => i.points.map((p) => p.week))),
  ].sort();
  const first = weeks[0];
  const last = weeks.at(-1);

  let kpis: ProgressViewModel["kpis"] = null;
  if (first && last) {
    const reportDay = input.reportAt ? isoDay(input.reportAt) : null;
    const events =
      reportDay && reportDay >= first && reportDay <= addDays(last, 6)
        ? [{ date: reportDay, label: "your report was built" }]
        : [];
    const collapsed = weeks.length < MIN_TREND_WEEKS;
    const groups = GROUPS.map((g) => grid(g, byMetric, collapsed));
    kpis = {
      range: { from: first, to: last },
      events,
      weeks: weeks.length,
      collapsed,
      rating: grid(
        { id: "rating", title: "Rating", metrics: ["rating"], nUnit: GAMES },
        byMetric,
        collapsed,
      ),
      groups,
      keyGroup: groups.find((g) => g.items.length > 0)?.id ?? null,
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

  // Rust reviews a week with games on the Monday after it (G2, `reviewed_week`). The first review
  // still to come is the earliest such week whose Monday is today or later.
  const played = input.trends
    .flatMap((t) => t.points.filter((p) => p.n > 0).map((p) => weekMonday(p.week)))
    .filter((w): w is string => w != null)
    .sort()
    .find((w) => addDays(w, 7) >= today);
  const firstReview: FirstReview =
    input.weekly.length === 0 && played ? { week: played, due: addDays(played, 7) } : null;

  return {
    basis: input.basis,
    clipped: input.clipped === true,
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
 * One key per page: the first group with a chart shows KpiTrendGrid's key and report list; the
 * rating and the other groups hide theirs and say the one thing that differs (their threshold).
 * Depends on the grid's first child being the key block (hand-off: a `showKey` prop in @ci/ui).
 */
const NO_KEY = "[&>div:first-child]:hidden";

function ReportNote({ events }: { events: { date: string; label: string }[] }) {
  const ev = events[0];
  if (!ev) return null;
  return (
    <>
      {" "}
      Dashed rule 1 is the day <time dateTime={ev.date}>{dayText(ev.date)}</time> {ev.label}.
    </>
  );
}

function EarlyList({ reads }: { reads: EarlyRead[] }) {
  if (reads.length === 0) return null;
  return (
    <ul className="m-0 flex list-none flex-col p-0 [font-variant-numeric:lining-nums]">
      {reads.map((r) => (
        <li
          key={r.metric}
          data-early={r.metric}
          className="flex flex-col gap-0.5 border-b border-rule py-2.5 last:border-b-0 md:flex-row md:gap-6"
        >
          <span className="flex flex-col gap-0.5 md:w-64 md:shrink-0">
            <span className="flex items-center gap-1 text-base text-ink">
              {r.label}
              {r.explain ? <ExplainPopover entry={r.explain} /> : null}
            </span>
            {r.explain ? <span className="text-sm text-ink-soft">{r.explain.meaning}</span> : null}
          </span>
          <span className="text-sm text-ink-soft md:pt-0.5">{r.text}</span>
        </li>
      ))}
    </ul>
  );
}

const sheet = "rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:py-8";

type Kpis = NonNullable<ProgressViewModel["kpis"]>;

/** Fewer than `MIN_TREND_WEEKS` weeks: one note and every metric's early read, no empty charts. */
function CollapsedTrends({ kpis }: { kpis: Kpis }) {
  return (
    <Section
      id="rating-heading"
      title="Rating and trends"
      lede="Your recorded rating; it moves for many reasons."
    >
      <div data-surface="sheet" className={`${sheet} flex flex-col gap-6`}>
        <p className="m-0 max-w-2xl text-base text-ink [font-variant-numeric:lining-nums]">
          Trends appear after {MIN_TREND_WEEKS} weeks of games. You have {kpis.weeks} so far.
        </p>
        {[kpis.rating, ...kpis.groups].map((g) => (
          <div key={g.id} className="flex flex-col gap-1">
            {g.id === "rating" ? null : (
              <h3 className="m-0 text-sm font-medium text-ink-soft">{g.title}</h3>
            )}
            <EarlyList reads={g.early} />
          </div>
        ))}
      </div>
    </Section>
  );
}

function Trends({ kpis }: { kpis: Kpis }) {
  const { range, events } = kpis;
  return (
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
          {kpis.rating.items.length > 0 ? (
            <>
              <KpiTrendGrid
                items={kpis.rating.items}
                range={range}
                events={events}
                minN={kpis.rating.minN}
                nUnit={kpis.rating.nUnit}
                headingLevel={3}
                className={`lg:col-span-3 ${NO_KEY}`}
              />
              <p className="m-0 max-w-md text-sm text-ink-faint lg:col-span-2">
                The line joins weeks with a recorded rating; a break is a week without games.
                <ReportNote events={events} />
              </p>
            </>
          ) : (
            <div className="lg:col-span-5">
              <EarlyList reads={kpis.rating.early} />
            </div>
          )}
        </div>
      </Section>

      <Section
        id="kpi-heading"
        title="How you play"
        lede="Each week's value with its stored 80% range, on the same weeks as your rating."
      >
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {kpis.groups.map((g, i) => {
            const keyed = g.id === kpis.keyGroup;
            return (
              <section
                key={g.id}
                data-surface="sheet"
                className={`${sheet} flex flex-col gap-4 ${i === 0 ? "lg:col-span-2" : i === 1 ? "lg:row-span-2" : ""}`}
                aria-labelledby={`kpi-${g.id}`}
              >
                <div className="flex flex-col gap-1">
                  <h3 id={`kpi-${g.id}`} className="m-0 font-serif text-xl font-semibold text-ink">
                    {g.title}
                  </h3>
                  {g.items.length > 0 && !keyed ? (
                    <p className="m-0 text-sm text-ink-faint">
                      Marks as in the key above; hollow weeks had fewer than {g.minN} {g.caption}.
                    </p>
                  ) : null}
                </div>
                {g.items.length > 0 ? (
                  <KpiTrendGrid
                    items={g.items}
                    range={range}
                    events={events}
                    minN={g.minN}
                    nUnit={g.nUnit}
                    headingLevel={4}
                    className={keyed ? undefined : NO_KEY}
                  />
                ) : null}
                {g.early.length > 0 ? (
                  <div className="flex flex-col gap-1">
                    {g.items.length > 0 ? (
                      <h4 className="m-0 text-sm font-medium text-ink-soft">
                        Not enough games for a trend yet
                      </h4>
                    ) : null}
                    <EarlyList reads={g.early} />
                  </div>
                ) : null}
              </section>
            );
          })}
        </div>
      </Section>
    </>
  );
}

export function ProgressView({
  view,
  scopeSwitch,
  compare,
}: {
  view: ProgressViewModel;
  scopeSwitch?: ReactNode;
  /** Period A against period B for the game-level stats, when the URL asks for it (T3). */
  compare?: ReactNode;
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

      {compare}

      <Section
        id="before-after-heading"
        title="Before and after"
        lede="Your rate either side of each change the detector has called."
      >
        <div data-surface="sheet" className={sheet}>
          <BeforeAfter rows={view.beforeAfter} />
        </div>
      </Section>

      {view.kpis === null ? (
        <Section id="rating-heading" title="Rating and trends">
          <p className="max-w-2xl text-base text-ink-soft">
            {view.clipped
              ? "No analysed week falls in these dates. Widen the period to see the trends."
              : "Weekly trends appear once a week of your games is analysed."}
          </p>
        </Section>
      ) : view.kpis.collapsed ? (
        <CollapsedTrends kpis={view.kpis} />
      ) : (
        <Trends kpis={view.kpis} />
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
