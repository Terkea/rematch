import { AreaScore, AreaScoreKey } from "@ci/ui/area-score";
import { AiDisclosure, AiLabel, CoachState, SegmentText } from "@ci/ui/coach";
import { cn } from "@ci/ui/lib";
import { Button, touchTarget } from "@ci/ui/primitives";
import {
  dayText,
  ExemplarBoard,
  ExplainPopover,
  KpiTrendGrid,
  type KpiTrendItem,
  WeaknessRow,
  WeaknessRowKey,
  weaknessScaleMax,
} from "@ci/ui/profile";
import { WeaknessTimeline } from "@ci/ui/timeline";
import Link from "next/link";
import type { ReactNode } from "react";
import { CoachScopeSwitch } from "../../../../../components/coach-scope-switch";
import { coachChatLive } from "../../../../../lib/coach-chat";
import { areaMeaning } from "../../../../../lib/explain";
import { weeklyPoints } from "../../../../../lib/iso-week";
import { segmentHref } from "../../../../../lib/segment-href";
import { coachHref } from "../../../../../server/coach/anchor";
import type { CoachStatus } from "../../../../../server/coach/status";
import type { ReportView } from "../../../../../server/coaching";
import type { NarrativeItem } from "../../../../../server/coaching/types";
import {
  AREA_GLOSSARY,
  KPI_GLOSSARY,
  seriesGlossary,
} from "../../../../../server/profile/glossary";
import type {
  Area,
  AreaView as AreaReadModel,
  CoachScope,
  KpiCurrent,
  KpiTrend,
  TimelineSeries,
  WeaknessSummary,
} from "../../../../../server/profile/types";
import { EmptySheet } from "../../../_shell/page-heading";
import { SectionHead } from "../../_overview/section-head";
import { numbersOnlyCopy } from "../../report/notes-off";
import { type ScoreBasis, shortfallUnit } from "./basis";
import { ScrollCue } from "./scroll-cue";

// The skill-area page's layout, shared by all seven areas (one layout, no card grid, B4): a
// heading with the area tabs, then ruled sections down one column. Score and the coach's note
// sit side by side from 1024px; measures, weaknesses and positions take the full width.

export type AreaPageModel = {
  area: Area;
  scope: CoachScope;
  score: AreaReadModel["score"];
  /** What the score's n counts (Rust's thinnest component), or null without a score row. */
  basis: ScoreBasis | null;
  /** The area's current (last-50) rows, which name the unit of the report's shortfall. */
  areaKpis: KpiCurrent[];
  kpis: KpiTrend[];
  weaknesses: { summary: WeaknessSummary; trend: (number | null)[] }[];
  timeline: TimelineSeries[];
  report: ReportView;
  /** Today in UTC ("2026-11-22"), the right end of the timeline. */
  today: string;
  /** Whether a coach could write now (`ai_status`, ADR 0078). */
  coach: CoachStatus;
  /** The render's clock, for when the next rebuild is available. */
  now: Date;
  /** The shared filter bar (T3); it holds the time control. */
  filterBar?: ReactNode;
  /** Whether the scope switch is still needed beside the bar (more than one linked account). */
  accountSwitch?: boolean;
};

const ORDER: readonly Area[] = [
  "openings",
  "middlegame",
  "endgame",
  "tactics",
  "calculation",
  "time",
  "mindset",
];

const TITLE: Record<Area, string> = {
  openings: "Openings",
  middlegame: "Middlegame",
  endgame: "Endgame",
  tactics: "Tactics",
  calculation: "Calculation",
  time: "Time",
  mindset: "Mindset",
};

/** The area in running text: "your endgame", "your time management". */
const NOUN: Record<Area, string> = {
  openings: "openings",
  middlegame: "middlegame",
  endgame: "endgame",
  tactics: "tactics",
  calculation: "calculation",
  time: "time management",
  mindset: "mindset",
};

/** "an endgame score". Mindset has none by design (R8). */
const SCORE_NAME: Record<Exclude<Area, "mindset">, string> = {
  openings: "an openings score",
  middlegame: "a middlegame score",
  endgame: "an endgame score",
  tactics: "a tactics score",
  calculation: "a calculation score",
  time: "a time-management score",
};

const TIME_CLASS: Record<CoachScope["timeClass"], string> = {
  bullet: "bullet",
  blitz: "blitz",
  rapid: "rapid",
  classical: "classical",
  daily: "daily",
};
const PLATFORM: Record<CoachScope["platform"], string> = {
  lichess: "lichess",
  chesscom: "chess.com",
};

/** Weeks with fewer games than this are drawn hollow, off the line. */
const WEEK_MIN_GAMES = 5;
const MAX_EXAMPLES = 3;
const LINING = { fontVariantNumeric: "lining-nums" } as const;

const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);
const games = (n: number) => `${count(n)} ${n === 1 ? "game" : "games"}`;
const finite = (v: number | null | undefined): v is number =>
  typeof v === "number" && Number.isFinite(v);

function scopeQuery(scope: CoachScope): string {
  const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
  // The colour filter travels with the scope (ADR 0082).
  if (scope.side === "white" || scope.side === "black") q.set("color", scope.side);
  return q.toString();
}

const areaHref = (area: Area, scope: CoachScope) => `/app/areas/${area}?${scopeQuery(scope)}`;

export function AreaView({ model }: { model: AreaPageModel }) {
  const { area, scope } = model;
  const option = scope.options.find(
    (o) => o.accountId === scope.accountId && o.timeClass === scope.timeClass,
  );
  return (
    <div className="flex flex-col gap-10">
      <header className="flex flex-col gap-5">
        <AreaTabs current={area} scope={scope} />
        <div className="flex max-w-3xl flex-col gap-2">
          <div className="flex items-center gap-1.5">
            <h1 className="font-serif text-3xl font-semibold text-ink md:text-4xl">
              {TITLE[area]}
            </h1>
            <ExplainPopover entry={AREA_GLOSSARY[area]} />
          </div>
          <p className="text-lg text-ink-soft">{AREA_GLOSSARY[area].meaning}</p>
          <p data-slot="meaning" className="m-0 text-base text-ink" style={LINING}>
            {areaMeaning({
              score: model.score?.value ?? null,
              interval: null,
              n: model.score?.n ?? 0,
              unscored: area === "mindset",
            })}
          </p>
          {option ? (
            <p className="text-sm text-ink-faint" style={LINING}>
              From {count(option.analysedGames)} analysed {TIME_CLASS[scope.timeClass]}{" "}
              {option.analysedGames === 1 ? "game" : "games"} on {PLATFORM[scope.platform]}
            </p>
          ) : null}
        </div>
        {model.accountSwitch !== false || !model.filterBar ? (
          <ScrollCue>
            <CoachScopeSwitch scope={scope} basePath={`/app/areas/${area}`} />
          </ScrollCue>
        ) : null}
        {model.filterBar}
      </header>

      <div className="grid grid-cols-1 gap-x-12 gap-y-10 lg:grid-cols-12">
        <Section id="area-score" title="Score" className="lg:col-span-7">
          <ScorePart model={model} />
        </Section>
        <Section id="area-note" title="Coach's note" className="lg:col-span-5">
          <NotePart model={model} />
        </Section>
      </div>

      <Section id="area-measures" title="Measures">
        <MeasuresPart model={model} />
      </Section>

      <Section id="area-weaknesses" title="Weaknesses">
        <WeaknessesPart model={model} />
      </Section>

      <Section id="area-examples" title="Example positions">
        <ExamplesPart model={model} />
      </Section>
    </div>
  );
}

function Section({
  id,
  title,
  className,
  children,
}: {
  id: string;
  title: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={id} className={cn("flex min-w-0 flex-col gap-5", className)}>
      <SectionHead id={id} title={title} />
      {children}
    </section>
  );
}

/** The seven areas as links, in a pill track like the app's tabs; the current one is a chip. */
function AreaTabs({ current, scope }: { current: Area; scope: CoachScope }) {
  return (
    <ScrollCue>
      <nav
        aria-label="Skill areas"
        data-scroller
        className="relative max-w-full overflow-x-auto pointer-coarse:-my-1.5 pointer-coarse:py-1.5"
      >
        <ul className="m-0 inline-flex w-fit list-none items-center gap-0.5 rounded-pill bg-well p-1">
          {ORDER.map((a) => (
            <li key={a}>
              <Link
                href={areaHref(a, scope)}
                aria-current={a === current ? "page" : undefined}
                className={cn(
                  "relative inline-flex h-8 shrink-0 items-center rounded-pill px-4 font-sans text-sm font-medium whitespace-nowrap",
                  "transition-colors duration-(--dur-quick) ease-out",
                  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                  a === current
                    ? "bg-sheet font-semibold text-ink inset-ring-1 inset-ring-rule"
                    : "text-ink-soft hover:text-ink",
                  touchTarget,
                )}
              >
                {TITLE[a]}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
    </ScrollCue>
  );
}

// ---- Score ----

function ScorePart({ model }: { model: AreaPageModel }) {
  const { area, scope, score, basis, report } = model;
  if (area === "mindset") {
    return (
      <Quiet title="Mindset is coaching advice, not a score">
        Session habits don't compare cleanly against other players, so there is no percentile here.
        Your coach's note and the measures below describe it instead.
      </Quiet>
    );
  }
  const tc = TIME_CLASS[scope.timeClass];
  if (score && finite(score.value)) {
    // Rust's area n is the smallest sample among its components: games for a mean, chances for
    // a rate. It is named in that unit, or with no unit when no component's row matches it.
    const unit =
      basis?.unit === "chances"
        ? { one: "chance", many: "chances" }
        : basis?.unit === "games"
          ? undefined
          : { one: "in its thinnest measure", many: "in its thinnest measure" };
    return (
      <div className="flex flex-col gap-4">
        <AreaScoreKey next={false} interval={false} />
        <AreaScore
          area={area}
          score={Math.round(score.value)}
          band={{ median: 50 }}
          basis={{ games: score.n, timeClass: tc, ...(unit ? { unit } : {}) }}
          interval={null}
        />
        <p className="m-0 max-w-prose text-sm text-ink-soft">
          A percentile against players in your rating band, so the band median is 50 by definition.{" "}
          {basis?.label
            ? `The count is its thinnest measure, ${basis.label}.`
            : "The count is its thinnest measure."}
        </p>
      </div>
    );
  }
  // The report's shortfall only when the report is at least as new as the live row.
  const row = report.payload?.computed.areaScores.find((a) => a.area === area) ?? null;
  const current = !score || (report.createdAt !== null && report.createdAt >= score.computedAt);
  const needMore = current && row && finite(row.needMore) && row.needMore > 0 ? row.needMore : null;
  if (!score && needMore === null) {
    return (
      <Quiet title="Not enough games for a score yet">
        Your profile hasn't scored this area yet.
      </Quiet>
    );
  }
  const n = score ? score.n : (row?.n ?? 0);
  let sample: string;
  if (n <= 0) sample = "None of its measures has a sample yet.";
  else if (basis?.label && basis.unit) {
    const word =
      basis.unit === "chances" ? (n === 1 ? "chance" : "chances") : n === 1 ? "game" : "games";
    sample = `Its thinnest measure, ${basis.label}, has ${count(n)} ${word} so far.`;
  } else sample = `Its thinnest measure has ${count(n)} so far.`;
  // The shortfall is in its own component's unit; one that matches no measure is stated bare.
  const unit = needMore === null ? null : shortfallUnit(area, needMore, model.areaKpis);
  if (needMore !== null && unit === null) {
    sample = `${sample} Its thinnest measure needs ${count(needMore)} more.`;
  }
  const word =
    unit === "chances"
      ? needMore === 1
        ? "chance"
        : "chances"
      : needMore === 1
        ? "game"
        : "games";
  return (
    <Quiet
      title={
        needMore !== null && unit !== null
          ? `Need ${count(needMore)} more ${word} for ${SCORE_NAME[area]}`
          : "Not enough games for a score yet"
      }
    >
      {sample}
    </Quiet>
  );
}

/** A stated gap: a serif line and a quiet explanation, never a number standing in for one. */
function Quiet({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex max-w-prose flex-col gap-1.5" style={LINING}>
      <p className="m-0 font-serif text-xl font-semibold text-ink">{title}</p>
      <p className="m-0 text-sm text-ink-soft">{children}</p>
    </div>
  );
}

// ---- Coach's note ----

function NotePart({ model }: { model: AreaPageModel }) {
  const { area, scope, report } = model;
  const payload = report.payload;
  const anchor = payload
    ? ({ kind: "report_section", reportId: payload.reportId, section: "areas" } as const)
    : null;
  const ask = coachChatLive() ? (
    <div>
      <Button
        asChild
        variant="outline"
        className={cn("h-auto min-h-10 py-2 text-left whitespace-normal", touchTarget)}
      >
        <Link href={coachHref(anchor, scope)}>Ask your coach about your {NOUN[area]}</Link>
      </Button>
    </div>
  ) : null;
  return (
    <div className="flex flex-col gap-5">
      <Note area={area} report={report} scope={scope} coach={model.coach} now={model.now} />
      {ask}
    </div>
  );
}

function Note({
  area,
  report,
  scope,
  coach,
  now,
}: {
  area: Area;
  report: ReportView;
  scope: CoachScope;
  coach: CoachStatus;
  now: Date;
}) {
  const { payload, state } = report;
  const building = state.kind === "building";
  const writing = (
    <CoachState
      kind="writing"
      title="Your coach is writing this section"
      body="It appears here as soon as it's written."
    />
  );
  if (!payload) {
    if (building) return writing;
    return (
      <CoachState
        kind="empty"
        title={`Your coach's note on your ${NOUN[area]} comes with your first report`}
        action={{ label: "Open your report", href: `/app/report?${scopeQuery(scope)}` }}
      />
    );
  }
  const section = payload.sections.areas;
  if (section === null) {
    return (
      <CoachState
        kind="unavailable"
        title="This note isn't shown"
        body={`Your coach's note on your ${NOUN[area]} couldn't be checked against your games, so it isn't shown.`}
      />
    );
  }
  if (section === undefined || section.areas == null) {
    if (building) return writing;
    const ready = state.kind === "ready" ? state : null;
    const aiOff = ready?.aiOff ?? null;
    if (aiOff === "budget") {
      return (
        <CoachState
          kind="budget"
          title="Your coach's notes are paused"
          body="Your coach has reached this month's spending limit, so this report shows the computed numbers only."
        />
      );
    }
    // Fix D (N2): the report banner's reading, so both pages say the same thing.
    if (aiOff === "no_api_key" || ready?.status === "computed_only") {
      const copy = numbersOnlyCopy({ coach, nextManualAt: report.nextManualAt, now });
      return <CoachState kind={copy.kind} title={copy.title} body={copy.body} />;
    }
  }
  const item: NarrativeItem | undefined = section?.areas?.items.find((i) => i.area === area);
  if (!item) {
    return (
      <CoachState
        kind="empty"
        title={`Your coach didn't write a note on your ${NOUN[area]} in this report`}
      />
    );
  }
  return (
    <div className="flex flex-col gap-4">
      <SegmentText
        segments={item.segments}
        hrefFor={segmentHref}
        className="max-w-prose font-serif text-prose text-ink"
      />
      {payload.aiDisclosure ? (
        <AiDisclosure disclosure={payload.aiDisclosure} compact />
      ) : (
        <AiLabel />
      )}
    </div>
  );
}

// ---- Measures ----

function MeasuresPart({ model }: { model: AreaPageModel }) {
  const { kpis, report } = model;
  const charted = kpis.filter((k) => k.current?.sufficient === true);
  const thin = kpis.filter((k) => k.current?.sufficient !== true);
  if (kpis.length === 0) {
    return <p className="m-0 text-sm text-ink-soft">No measures are defined for this area yet.</p>;
  }
  const items: KpiTrendItem[] = charted.map((k) => ({
    metric: k.metric,
    label: k.label,
    unit: k.unit,
    explain: KPI_GLOSSARY[k.metric] ?? null,
    // The read model keys weeks as "2026-W46"; the chart plots days (B2).
    points: weeklyPoints(k.points),
  }));
  const weeks = items.flatMap((k) => k.points.map((p) => p.week)).sort();
  const range = { from: weeks[0] ?? model.today, to: weeks.at(-1) ?? model.today };
  const reportDay = report.createdAt?.toISOString().slice(0, 10) ?? null;
  const events =
    reportDay && reportDay >= range.from && reportDay <= range.to
      ? [{ date: reportDay, label: "Coaching report" }]
      : [];
  return (
    <div className="flex flex-col gap-8">
      {items.length > 0 ? (
        <KpiTrendGrid
          items={items}
          range={range}
          events={events}
          minN={WEEK_MIN_GAMES}
          headingLevel={3}
        />
      ) : null}
      {thin.length > 0 ? (
        <div className="flex max-w-3xl flex-col gap-2">
          <h3 className="m-0 text-base font-medium text-ink">
            {items.length > 0 ? "Still gathering games" : "Not enough games for these yet"}
          </h3>
          <ul className="m-0 flex list-none flex-col p-0">
            {thin.map((k) => (
              <ThinMeasure key={k.metric} kpi={k} />
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

/** A measure below its minimum: its n and the shortfall, never its value. */
function ThinMeasure({ kpi }: { kpi: KpiTrend }) {
  const c = kpi.current;
  const explain = KPI_GLOSSARY[kpi.metric] ?? null;
  let status: string;
  if (!c) status = "Not measured yet";
  else {
    const so = c.n > 0 ? `${games(c.n)} so far. ` : "No games counted yet. ";
    const need = finite(c.needMore) && c.needMore > 0 ? c.needMore : null;
    status =
      need !== null
        ? `${so}Need ${count(need)} more ${need === 1 ? "game" : "games"}`
        : `${so}Not measured yet`;
  }
  return (
    <li
      className="flex flex-col gap-0.5 border-b border-rule py-2.5 last:border-b-0 sm:flex-row sm:items-baseline sm:gap-6"
      style={LINING}
    >
      <span className="flex flex-col gap-0.5 sm:w-72 sm:shrink-0">
        <span className="flex items-center gap-1 text-ink">
          {kpi.label}
          {explain ? <ExplainPopover entry={explain} /> : null}
        </span>
        {explain ? <span className="text-sm text-ink-soft">{explain.meaning}</span> : null}
      </span>
      <span className="text-sm text-ink-soft">{status}</span>
    </li>
  );
}

// ---- Weaknesses ----

function WeaknessesPart({ model }: { model: AreaPageModel }) {
  const { area, scope, weaknesses, timeline } = model;
  if (weaknesses.length === 0) {
    return (
      <p className="m-0 max-w-prose text-sm text-ink-soft">
        No weakness in your {NOUN[area]} has been measured yet. They appear here as your analysed
        games give the profile enough chances to judge.
      </p>
    );
  }
  const scaleMax = weaknessScaleMax(weaknesses.map((w) => w.summary));
  const from = timeline.flatMap((t) => t.segments.map((s) => s.from.slice(0, 10))).sort()[0];
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-3">
        <WeaknessRowKey />
        <ul className="m-0 flex list-none flex-col p-0">
          {weaknesses.map(({ summary: w, trend }) => (
            <li key={w.seriesKey} className="border-b border-rule last:border-b-0">
              <WeaknessRow
                label={w.label}
                state={w.state}
                rate={{ mean: w.rate.mean, low: w.rate.low, high: w.rate.high }}
                evidence={{
                  events: w.evidence.events,
                  chances: w.evidence.chances,
                  sufficient: w.evidence.sufficient,
                  needMore: w.evidence.needMore,
                }}
                band={{ median: w.band.median }}
                href={`/app/weaknesses/${encodeURIComponent(w.seriesKey)}?${scopeQuery(scope)}`}
                trend={trend}
                onset={w.onset ? w.onset.at.toISOString().slice(0, 10) : null}
                scaleMax={scaleMax}
                explain={seriesGlossary(w.seriesKey)}
                {...(scope.side === "white" || scope.side === "black" ? { side: scope.side } : {})}
              />
            </li>
          ))}
        </ul>
      </div>
      {timeline.length > 0 && from ? (
        <div className="flex flex-col gap-3">
          <h3 className="m-0 text-base font-medium text-ink">How they changed</h3>
          {scope.side === "white" || scope.side === "black" ? (
            <p className="m-0 text-sm text-ink-soft">
              States are called on all your games together, so this timeline covers both colours.
            </p>
          ) : null}
          <WeaknessTimeline
            series={timeline}
            range={{ from, to: model.today }}
            label={`${TITLE[area]} weaknesses over time`}
          />
        </div>
      ) : null}
    </div>
  );
}

// ---- Example positions ----

function ExamplesPart({ model }: { model: AreaPageModel }) {
  const { area, report } = model;
  const payload = report.payload;
  if (!payload) {
    return (
      <p className="m-0 text-sm text-ink-soft">
        Example positions appear once your first report is built.
      </p>
    );
  }
  const examples = payload.computed.weaknesses
    .filter((w) => w.area === area)
    .sort((a, b) => a.rank - b.rank)
    .flatMap((w) => w.exemplars.map((e) => ({ label: w.label, e })))
    .slice(0, MAX_EXAMPLES);
  const day = report.createdAt ? report.createdAt.toISOString() : payload.generatedAt;
  const from = (
    <p className="m-0 text-sm text-ink-soft">
      Examples from your report of <time dateTime={day.slice(0, 10)}>{dayText(day)}</time>
    </p>
  );
  if (examples.length === 0) {
    return (
      <div className="flex flex-col gap-1">
        {from}
        <p className="m-0 text-sm text-ink-soft">
          That report picked no positions for your {NOUN[area]}.
        </p>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-6">
      {from}
      <ol className="m-0 flex list-none flex-wrap gap-x-8 gap-y-10 p-0">
        {examples.map(({ label, e }) => (
          <li key={`${e.gameId}:${e.ply}`} className="flex flex-col gap-3">
            <div className="flex flex-col">
              <span className="font-medium text-ink">{label}</span>
              <span className="text-sm text-ink-faint" style={LINING}>
                Played <time dateTime={e.playedAt.slice(0, 10)}>{dayText(e.playedAt)}</time>
              </span>
            </div>
            <ExemplarBoard
              // ban-lint-allow TOK2: three boards share one row of the 1088px column; a phone keeps the page token
              className="lg:[--board-size:20rem]"
              fen={e.fen}
              played={e.played}
              best={e.best}
              refutation={e.refutationPv[0] ?? null}
              caption={{ winBefore: e.winBefore, winAfter: e.winAfter }}
            />
            <Link
              href={`/app/games/${encodeURIComponent(e.gameId)}?ply=${e.ply}`}
              className={cn(
                "relative w-fit text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                touchTarget,
              )}
            >
              Open this game at the move
            </Link>
          </li>
        ))}
      </ol>
    </div>
  );
}

// ---- No scope ----

export function AreaEmpty({ area }: { area: Area }) {
  return (
    <div className="flex flex-col gap-8">
      <h1 className="font-serif text-3xl font-semibold text-ink">{TITLE[area]}</h1>
      <EmptySheet
        action={
          <Button asChild variant="outline">
            <Link href="/app/games">Games</Link>
          </Button>
        }
      >
        Your skill areas appear once some of your games are analysed.
      </EmptySheet>
    </div>
  );
}
