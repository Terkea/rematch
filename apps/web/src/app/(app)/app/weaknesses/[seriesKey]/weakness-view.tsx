import { AiDisclosure, type Segment, SegmentText } from "@ci/ui/coach";
import { CaretLeftIcon, Icon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import {
  countsText,
  dayText,
  KpiTrendGrid,
  type KpiTrendItem,
  likelyText,
  NO_BENCHMARK_TEXT,
  NO_COLOUR_BENCHMARK,
  NO_COLOUR_BENCHMARK_TEXT,
  NOT_COMPARED_LABEL,
  needMoreText,
  probablyAbove,
  shareRangeText,
  shareText,
  weaknessRateText,
  weaknessScaleMax,
} from "@ci/ui/profile";
import { stateStyle, WeaknessTimeline } from "@ci/ui/timeline";
import Link from "next/link";
import type { ReactNode } from "react";
import { MorphArrival } from "../../../../../components/shared-morph";
import { coachChatLive } from "../../../../../lib/coach-chat";
import { howOftenText, levelText, trendText } from "../../../../../lib/explain";
import type { ChangeNoteView } from "../../../../../server/coaching";
import type { DayRange } from "../../../../../server/filters/params";
import type { CoachScope, WeaknessDetail } from "../../../../../server/profile";
import {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  benchmarkGroupLabel,
  benchmarkLabel,
  peerRateText,
} from "../../../../../server/profile/benchmark-text";
import { type GlossaryEntry, seriesGlossary } from "../../../../../server/profile/glossary";
import { ESTIMATE_WINDOW_TEXT } from "../../../../../server/profile/window";
import { clipWeekly } from "../../_filters/clip";
import { SectionHead } from "../../_overview/section-head";
import { scopeQuery, WEEKLY_MIN_CHANCES } from "../queries";
import { BandKey, morphName } from "../weaknesses-view";
import { Examples } from "./exemplars";

// `/app/weaknesses/[seriesKey]` (M5b Task 7, rewritten as a story): one weakness, top down. The
// header is the shared element the list row morphs into; below it, what the weakness is (the
// glossary), how often it happens to the player, whether it is getting better or worse, examples
// from their own games and what to do. The charts (where the estimate stands, its state over
// time, the weekly series and the latest change note) sit under "Details", intact.

const AREA_LABEL: Record<WeaknessDetail["area"], string> = {
  openings: "Openings",
  middlegame: "Middlegame",
  endgame: "Endgame",
  tactics: "Tactics",
  calculation: "Calculation",
  time: "Time",
  mindset: "Mindset",
};
const CHANCES = { one: "chance", many: "chances" };
const LINING = { fontVariantNumeric: "lining-nums" } as const;
const NUMERALS = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
const DAY = 86_400_000;
const isoDay = (d: Date) => d.toISOString().slice(0, 10);

/** The Monday of an ISO week ("2026-W45" to "2026-11-02"), or null for anything else. */
export function isoWeekMonday(week: string): string | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(week);
  if (!m) return null;
  const year = Number(m[1]);
  const n = Number(m[2]);
  if (n < 1 || n > 53) return null;
  const jan4 = Date.UTC(year, 0, 4);
  const monday1 = jan4 - ((new Date(jan4).getUTCDay() + 6) % 7) * DAY;
  return isoDay(new Date(monday1 + (n - 1) * 7 * DAY));
}

/** "Started around 14 Oct 2026, between 2 Oct and 20 Oct", from the onset row only. */
export function onsetText(onset: NonNullable<WeaknessDetail["onset"]>): string {
  const at = isoDay(onset.at);
  const year = at.slice(0, 4);
  const short = (d: Date) => {
    const iso = isoDay(d);
    return iso.slice(0, 4) === year ? dayText(iso).replace(/ \d{4}$/, "") : dayText(iso);
  };
  const head = `Started around ${dayText(at)}`;
  return onset.low && onset.high
    ? `${head}, between ${short(onset.low)} and ${short(onset.high)}`
    : head;
}

function weeklyItem(detail: WeaknessDetail): KpiTrendItem {
  return {
    metric: detail.seriesKey,
    label: "Weekly rate",
    unit: "rate",
    nUnit: CHANCES,
    points: detail.weekly.flatMap((w) => {
      const week = isoWeekMonday(w.week);
      if (!week) return [];
      return [
        {
          week,
          value: w.rate?.value ?? null,
          low: w.rate?.low ?? null,
          high: w.rate?.high ?? null,
          n: w.chances,
        },
      ];
    }),
  };
}

/** The stored interval on a 0-to-max scale, with the band median as a pencil tick below. */
function IntervalScale({
  rate,
  median,
  early,
}: {
  rate: WeaknessDetail["rate"];
  median: number | null;
  /** An early read draws its range as a dashed outline and its estimate soft: provisional. */
  early: boolean;
}) {
  const max = weaknessScaleMax([{ rate, band: { median } }]);
  const at = (v: number) => `${Math.min(100, Math.max(0, (v / max) * 100)).toFixed(3)}%`;
  return (
    <div aria-hidden="true" className="flex flex-col gap-1">
      <div className="relative h-7 w-full">
        <div className="absolute inset-x-0 top-3 h-px bg-rule" />
        <div
          data-part="interval"
          className={
            early
              ? "absolute top-2 h-2.5 rounded-(--radius-board) border border-dashed border-ink-faint"
              : "absolute top-2 h-2.5 rounded-(--radius-board) bg-rule-strong"
          }
          style={{ left: at(rate.low), width: `calc(${at(rate.high)} - ${at(rate.low)})` }}
        />
        <div
          data-part="mean"
          className={`absolute top-0.5 h-5.5 w-1 -translate-x-1/2 rounded-full outline-2 outline-sheet ${early ? "bg-ink-soft" : "bg-ink"}`}
          style={{ left: at(rate.mean) }}
        />
        {median !== null && Number.isFinite(median) ? (
          <div
            className="absolute top-4 h-3 w-0.5 -translate-x-1/2 rounded-full bg-pencil"
            style={{ left: at(median) }}
          />
        ) : null}
      </div>
      <div className="flex justify-between text-xs text-ink-faint" style={LINING}>
        <span>0%</span>
        <span>{scaleText(max)}</span>
      </div>
    </div>
  );
}

/**
 * Where the band median sits against the stored range; only a clear miss says lower or higher.
 * Inside it, a stored confidence of 0.7 or more adds "though yours is probably higher" (ADR 0084).
 */
export function bandRelation(
  rate: { low: number; high: number },
  median: number,
  confidence?: number,
): string {
  if (median < rate.low) return "lower than your whole likely range";
  if (median > rate.high) return "higher than your whole likely range";
  if (probablyAbove(confidence)) {
    return `inside your likely range, though yours is probably higher (${likelyText(confidence)})`;
  }
  return "inside your likely range";
}

/** A scale end at whole-number precision where it is whole ("7%", "12.5%"), like "0%". */
const scaleText = (share: number) =>
  Number.isFinite(share) ? `${Number((share * 100).toFixed(1))}%` : "";

/** The early read's key: the same marks drawn provisional, as the strip draws them. */
function EarlyKey({ group }: { group: string }) {
  return (
    <ul
      aria-label="Key"
      className="m-0 flex list-none flex-wrap gap-x-5 gap-y-1 p-0 text-xs text-ink-soft"
    >
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="h-4 w-0.75 rounded-full bg-ink-soft" />
        Early estimate
      </li>
      <li className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className="h-2 w-5 rounded-(--radius-board) border border-dashed border-ink-faint"
        />
        80% range, provisional
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="mt-2 h-2.5 w-0.5 rounded-full bg-pencil" />
        {group}, median
      </li>
    </ul>
  );
}

function Header({
  detail,
  scope,
  askHref,
  range,
  filterBar,
}: Props & { askHref: string; range: DayRange; filterBar?: ReactNode | undefined }) {
  const early = !detail.evidence.sufficient;
  const colour = colourOf(scope);
  // Without a benchmark the stored state is no claim (bug hunt I1). A colour has no state of its
  // own: states are called on every game (ADR 0082), so it names the colour instead.
  const state = early
    ? "Early read"
    : colour
      ? detail.compared
        ? `Your ${colour} games`
        : `Your ${colour} games, ${NO_COLOUR_BENCHMARK.toLowerCase()}`
      : detail.compared
        ? stateStyle[detail.state].label
        : NOT_COMPARED_LABEL;
  const provisional = early || !detail.compared;
  return (
    <header className="flex flex-col gap-4">
      <Link
        href={`/app/weaknesses?${scopeQuery(scope, range)}`}
        className="inline-flex w-fit items-center gap-1 rounded-field text-sm text-ink-soft hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
      >
        <Icon icon={CaretLeftIcon} size={16} />
        All weaknesses
      </Link>
      <div className="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div className="flex min-w-0 flex-col gap-2">
          <h1 className="font-serif text-3xl font-semibold text-ink [overflow-wrap:anywhere]">
            {detail.label}
          </h1>
          <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-ink-soft">
            <span
              data-state={
                early ? "early" : colour ? "colour" : detail.compared ? detail.state : "uncompared"
              }
              className={
                provisional
                  ? "rounded-pill border border-dashed border-ink-faint px-2.5 py-0.5 font-medium"
                  : "rounded-pill bg-well px-2.5 py-0.5 font-medium text-ink"
              }
            >
              {state}
            </span>
            <span>{AREA_LABEL[detail.area]}</span>
          </p>
        </div>
        {coachChatLive() ? (
          <Button asChild variant="outline" className="w-fit shrink-0">
            <Link href={askHref}>Ask your coach about this</Link>
          </Button>
        ) : null}
      </div>
      {filterBar}
    </header>
  );
}

/** "White" or "Black" when the page reads one colour's rows, else null. */
const colourOf = (scope: { side?: string }) =>
  scope.side === "white" ? "White" : scope.side === "black" ? "Black" : null;

/** The reference group's name when nothing more specific is known. */
const DEFAULT_GROUP = "Players in your rating band";

function Standing({ detail, benchmark }: { detail: WeaknessDetail; benchmark: Benchmark | null }) {
  const group = benchmarkGroupLabel(benchmark) ?? DEFAULT_GROUP;
  const label = benchmarkLabel(benchmark);
  const early = !detail.evidence.sufficient;
  const none = detail.evidence.chances <= 0;
  const median = detail.compared ? detail.band.median : null;
  const peer = median === null ? null : peerRateText(benchmark, detail.seriesKey, median);
  const counts = `${countsText(detail.evidence)} ${ESTIMATE_WINDOW_TEXT}`;
  return (
    <section
      aria-label="Where it stands"
      data-surface="sheet"
      className="grid grid-cols-1 gap-x-10 gap-y-8 rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)] md:py-8"
    >
      <div className="flex flex-col gap-1">
        <span className="sr-only">
          {none
            ? `No estimate yet: ${counts}.`
            : `${early ? "Early read. " : ""}${weaknessRateText(detail.rate, detail.evidence, ESTIMATE_WINDOW_TEXT)}.`}
        </span>
        <span aria-hidden="true" className="text-sm text-ink-soft">
          {early ? "Early estimate" : "Estimated rate"}, % of chances
        </span>
        {none ? (
          <span aria-hidden="true" className="font-serif text-xl text-ink-soft">
            No chances yet
          </span>
        ) : (
          <span
            aria-hidden="true"
            className={
              early
                ? "font-serif text-xl leading-tight font-semibold text-ink-soft underline decoration-rule-strong decoration-dashed underline-offset-8"
                : "font-serif text-xl leading-tight font-semibold text-ink"
            }
            style={NUMERALS}
          >
            {shareText(detail.rate.mean)}
          </span>
        )}
        {none ? null : (
          <span aria-hidden="true" className="text-base text-ink-soft" style={LINING}>
            80% range {shareRangeText(detail.rate.low, detail.rate.high)}
          </span>
        )}
        <span aria-hidden="true" className="text-sm text-ink-faint" style={LINING}>
          {counts}
        </span>
        {early ? (
          <p className="mt-3 text-sm font-medium text-ink" style={LINING}>
            {detail.evidence.minEvidence === null
              ? "Early read: no evidence threshold for this series yet"
              : `Early read: ${needMoreText(detail.evidence.needMore)}`}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-4">
        {none ? null : (
          <>
            <IntervalScale rate={detail.rate} median={median} early={early} />
            {early ? (
              <EarlyKey group={group} />
            ) : (
              <BandKey benchmark={benchmark} className="text-xs" />
            )}
          </>
        )}
        <div className="flex flex-col gap-1 text-sm" style={LINING}>
          {median === null || !Number.isFinite(median) ? (
            <p className="text-ink-soft">{NO_BENCHMARK_TEXT}</p>
          ) : (
            <>
              {peer === null ? (
                <>
                  <p className="font-medium text-ink">Band median {shareText(median)}</p>
                  <p className="text-ink-soft">
                    {early || none
                      ? `${group}. A comparison waits for more chances.`
                      : `${group}, ${bandRelation(detail.rate, median, detail.confidence)}.`}
                  </p>
                </>
              ) : (
                <>
                  <p className="font-medium text-ink">{peer}</p>
                  <p className="text-ink-soft">
                    {early || none
                      ? "A comparison waits for more chances."
                      : `That is ${bandRelation(detail.rate, median, detail.confidence)}.`}
                  </p>
                </>
              )}
              {label === null ? null : (
                <p data-benchmark="peers" className="text-ink-soft">
                  {label}. {BENCHMARK_BIAS_TEXT}
                </p>
              )}
            </>
          )}
          <p className="mt-2 text-ink-soft">
            {detail.onset ? (
              <time dateTime={isoDay(detail.onset.at)}>{onsetText(detail.onset)}</time>
            ) : (
              "No start date detected yet"
            )}
          </p>
        </div>
      </div>
    </section>
  );
}

function Timeline({ detail }: { detail: WeaknessDetail }) {
  const t = detail.timeline;
  const starts = [t.segments[0]?.from, t.onset?.low].filter((d): d is string => !!d).sort();
  const from = starts[0];
  if (!from) {
    return <p className="text-base text-ink-soft">No state changes detected yet.</p>;
  }
  const last = t.segments.at(-1)?.from ?? from;
  const today = isoDay(new Date());
  const to = [last, isoDay(detail.updatedAt), today].sort().at(-1) ?? today;
  return (
    <div data-surface="sheet" className="rounded-sheet bg-sheet px-(--space-panel-pad) py-6">
      <WeaknessTimeline series={[t]} range={{ from, to }} />
    </div>
  );
}

function Weekly({ detail, range }: { detail: WeaknessDetail; range: DayRange }) {
  // The bar's dates clip whole weeks of the weekly rate (T3); the estimate keeps its window.
  const item = weeklyItem({ ...detail, weekly: clipWeekly(detail.weekly, range) });
  const first = item.points[0]?.week;
  const last = item.points.at(-1)?.week;
  if (!first || !last) {
    return (
      <p className="text-base text-ink-soft">
        {range.from || range.to ? "No weekly points in these dates." : "No weekly points yet."}
      </p>
    );
  }
  return (
    <div data-surface="sheet" className="rounded-sheet bg-sheet px-(--space-panel-pad) py-6">
      <KpiTrendGrid
        items={[item]}
        range={{ from: first, to: last }}
        minN={WEEKLY_MIN_CHANCES}
        nUnit={CHANCES}
        headingLevel={4}
      />
    </div>
  );
}

function Note({
  notes,
  hrefFor,
}: {
  notes: ChangeNoteView[];
  hrefFor: (s: Segment) => string | null;
}) {
  const ready = notes.find((n) => n.status === "ready" && n.item && n.aiDisclosure);
  const newerPending = notes
    .slice(0, ready ? notes.indexOf(ready) : notes.length)
    .some((n) => n.status === "pending");
  if (!ready?.item || !ready.aiDisclosure) {
    return newerPending ? (
      <p className="text-base text-ink-soft">A note on the latest change is being written.</p>
    ) : (
      <p className="text-base text-ink-soft">No note on a change yet.</p>
    );
  }
  return (
    <div
      data-surface="sheet"
      className="flex flex-col gap-4 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
    >
      <SegmentText
        segments={ready.item.segments}
        hrefFor={hrefFor}
        className="font-serif text-prose text-ink"
      />
      <AiDisclosure disclosure={ready.aiDisclosure} />
      {newerPending ? (
        <p className="text-sm text-ink-soft">A note on a newer change is being written.</p>
      ) : null}
    </div>
  );
}

type Props = { detail: WeaknessDetail; scope: CoachScope };
type Sides = { white: WeaknessDetail | null; black: WeaknessDetail | null };

const PROSE = "m-0 max-w-prose font-serif text-prose text-ink";
const SOFT = "m-0 max-w-prose text-base text-ink-soft";

/** One step of the story: a ruled head and its body, in reading order. */
function Step({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-4">
      <SectionHead id={id} title={title} />
      {children}
    </section>
  );
}

function WhatItIs({ entry }: { entry: GlossaryEntry | null }) {
  if (!entry) {
    return <p className={SOFT}>This series has no plain-language description yet.</p>;
  }
  return (
    <div className="flex flex-col gap-4">
      <p className={PROSE}>{entry.meaning}</p>
      <div className="flex max-w-prose flex-col gap-1">
        <h3 className="m-0 text-base font-medium text-ink">How it's measured</h3>
        <p className="m-0 text-base text-ink-soft" style={LINING}>
          {entry.measured}
        </p>
      </div>
      <div className="flex max-w-prose flex-col gap-1">
        <h3 className="m-0 text-base font-medium text-ink">Why it matters</h3>
        <p className="m-0 text-base text-ink-soft">{entry.whyItMatters}</p>
      </div>
    </div>
  );
}

/** "White" or "Black" for a side. */
const SIDE_NAME = { white: "White", black: "Black" } as const;

const LINK =
  "w-fit rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink";

/**
 * Both mode's White and Black block (A4): each colour's own stored estimate with its range and
 * chances, linked to that colour's view of this page. A colour without a row says so.
 */
function ByColour({
  detail,
  scope,
  range,
  sides,
}: {
  detail: WeaknessDetail;
  scope: CoachScope;
  range: DayRange;
  sides: Sides;
}) {
  return (
    <section
      aria-label="By colour"
      data-part="by-colour"
      className="grid max-w-prose grid-cols-1 gap-4 sm:grid-cols-2"
    >
      {(["white", "black"] as const).map((side) => {
        const d = sides[side];
        const name = SIDE_NAME[side];
        const href = `/app/weaknesses/${encodeURIComponent(detail.seriesKey)}?${scopeQuery({ ...scope, side }, range)}`;
        return (
          <div
            key={side}
            data-surface="sheet"
            className="flex flex-col gap-1 rounded-sheet bg-sheet px-(--space-panel-pad) py-4"
            style={LINING}
          >
            <h3 className="m-0 text-base font-medium text-ink">{name}</h3>
            {!d || d.evidence.chances <= 0 ? (
              <p className="m-0 text-sm text-ink-soft">
                No {name} games with a chance for this yet.
              </p>
            ) : (
              <>
                <p className="m-0 text-base text-ink">
                  <span className="font-serif text-xl font-semibold" style={NUMERALS}>
                    {shareText(d.rate.mean)}
                  </span>{" "}
                  of chances{d.evidence.sufficient ? "" : ", an early read"}
                </p>
                <p className="m-0 text-sm text-ink-soft">
                  80% range {shareRangeText(d.rate.low, d.rate.high)}, {countsText(d.evidence)}
                </p>
              </>
            )}
            <Link href={href} className={`${LINK} mt-1 text-sm`}>
              {name} games only
            </Link>
          </div>
        );
      })}
    </section>
  );
}

function HowOften({
  detail,
  benchmark,
  colour = null,
}: {
  detail: WeaknessDetail;
  benchmark: Benchmark | null;
  colour?: string | null;
}) {
  const early = !detail.evidence.sufficient;
  const none = detail.evidence.chances <= 0;
  const peer = detail.compared && benchmarkLabel(benchmark) !== null;
  return (
    <div className="flex flex-col gap-3" style={LINING}>
      {/* The story's main number at the KPI scale (critic 5); Details repeats it small. */}
      {none ? null : (
        <p className="m-0 flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <span
            data-part="story-rate"
            className={
              early
                ? "font-serif text-3xl leading-none font-semibold text-ink-soft"
                : "font-serif text-3xl leading-none font-semibold text-ink"
            }
            style={NUMERALS}
          >
            {shareText(detail.rate.mean)}
          </span>
          <span className="text-base text-ink-soft">
            {early ? "early estimate" : "of your chances"}, 80% range{" "}
            {shareRangeText(detail.rate.low, detail.rate.high)}
          </span>
        </p>
      )}
      <p className={PROSE}>{howOftenText(detail.evidence, ESTIMATE_WINDOW_TEXT)}</p>
      <p className={SOFT}>
        {colour && !detail.compared
          ? `${NO_COLOUR_BENCHMARK_TEXT}.`
          : levelText({
              rate: detail.rate,
              median: detail.band.median,
              compared: detail.compared,
              sufficient: detail.evidence.sufficient && !none,
              benchmark,
              confidence: detail.confidence,
              // A colour has no state of its own (ADR 0082): only its comparison speaks.
              ...(colour ? {} : { state: detail.state }),
            })}
        {/* One sentence names the players and their count; the bias follows it (critic 8). */}
        {peer ? ` ${BENCHMARK_BIAS_TEXT}` : null}
      </p>
    </div>
  );
}

function Trend({ detail, colour = null }: { detail: WeaknessDetail; colour?: string | null }) {
  if (colour) {
    // States and P_imp are called on every game (ADR 0082): a colour has no call of its own.
    return (
      <p className={PROSE}>
        Better or worse is called on all your games together, so your {colour} games have no call of
        their own. Set Colour to Both to see it.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2" style={LINING}>
      <p className={PROSE}>
        {trendText({
          state: detail.state,
          compared: detail.compared,
          sufficient: detail.evidence.sufficient,
          needMore: detail.evidence.needMore,
          pImp: detail.pImp,
        })}
      </p>
      {detail.onset ? (
        <p className={SOFT}>
          <time dateTime={isoDay(detail.onset.at)}>{onsetText(detail.onset)}</time>.
        </p>
      ) : null}
    </div>
  );
}

export function WeaknessView({
  detail,
  scope,
  notes,
  reviewIds,
  askHref,
  benchmark = null,
  range = { from: null, to: null },
  sides = null,
  filterBar,
  focusAction,
}: Props & {
  /** "Make this my focus" (ADR 0091), under what to do. */
  focusAction?: ReactNode;
  notes: ChangeNoteView[];
  /** What the comparison was made with (ADR 0080); null without a peer benchmark. */
  benchmark?: Benchmark | null;
  /** `games.id` by platform game id, for the examples and the note's moves. */
  reviewIds: Map<string, string>;
  askHref: string;
  /** The bar's dates (A4): they clip the weekly rate and travel on every link. */
  range?: DayRange;
  /** Both mode: each colour's own detail, for the White and Black block (A4). */
  sides?: Sides | null;
  /** The shared filter bar (colour, dates). */
  filterBar?: ReactNode;
}) {
  const hrefFor = (s: Segment): string | null => {
    if (s.kind !== "move" && s.kind !== "best" && s.kind !== "pv") return null;
    const id = reviewIds.get(s.gameId);
    return id ? `/app/games/${encodeURIComponent(id)}?ply=${s.ply}` : null;
  };
  const entry = seriesGlossary(detail.seriesKey);
  return (
    <MorphArrival
      name={morphName(detail.seriesKey)}
      className="flex flex-col gap-12"
      header={
        <Header
          detail={detail}
          scope={scope}
          askHref={askHref}
          range={range}
          filterBar={filterBar}
        />
      }
    >
      <Step id="what-heading" title="What this is">
        <WhatItIs entry={entry} />
      </Step>
      <Step id="often-heading" title="How often it happens to you">
        <HowOften detail={detail} benchmark={benchmark} colour={colourOf(scope)} />
        {/* With no colour row at all there is nothing to split: no pair of empty cards. */}
        {sides && (sides.white || sides.black) ? (
          <ByColour detail={detail} scope={scope} range={range} sides={sides} />
        ) : null}
      </Step>
      <Step id="trend-heading" title="Better or worse">
        <Trend detail={detail} colour={colourOf(scope)} />
      </Step>
      <Step id="examples-heading" title="Examples from your games">
        <Examples examples={detail.examples} reviewIds={reviewIds} />
      </Step>
      <Step id="todo-heading" title="What to do">
        {entry ? (
          <p className={PROSE}>{entry.tip}</p>
        ) : (
          <p className={SOFT}>No tip for this series yet. Your coach can suggest one.</p>
        )}
        {focusAction}
      </Step>

      <section aria-labelledby="details-heading" className="flex flex-col gap-8">
        <SectionHead id="details-heading" title="Details">
          <p className="m-0 text-sm text-ink-soft">
            The numbers behind the story: the estimate's range, its state over time and the
            week-by-week rate.
            {colourOf(scope)
              ? ` The range and the weekly rate are your ${colourOf(scope)} games'; the states over time are all your games'.`
              : ""}
          </p>
        </SectionHead>
        <Standing detail={detail} benchmark={benchmark} />

        <section aria-labelledby="timeline-heading" className="flex flex-col gap-4">
          <h3 id="timeline-heading" className="m-0 font-serif text-xl font-semibold text-ink">
            State over time
          </h3>
          <Timeline detail={detail} />
        </section>

        <div className="grid grid-cols-1 gap-x-10 gap-y-8 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
          <section aria-labelledby="weekly-heading" className="flex min-w-0 flex-col gap-4">
            <div className="flex flex-col gap-1">
              <h3 id="weekly-heading" className="m-0 font-serif text-xl font-semibold text-ink">
                Week by week
              </h3>
              <p className="m-0 text-sm text-ink-soft">
                The raw share of chances each week, with its 80% range. Weeks with fewer than{" "}
                {WEEKLY_MIN_CHANCES} chances are hollow and off the line.
                {range.from || range.to
                  ? " The dates set above clip these weeks only; the estimate and its range cover the last 6 months."
                  : ""}
              </p>
            </div>
            <Weekly detail={detail} range={range} />
          </section>

          <section aria-labelledby="note-heading" className="flex min-w-0 flex-col gap-4">
            <div className="flex flex-col gap-1">
              <h3 id="note-heading" className="m-0 font-serif text-xl font-semibold text-ink">
                What changed
              </h3>
              <p className="m-0 text-sm text-ink-soft">
                The coach's note on the latest detected change.
              </p>
            </div>
            <Note notes={notes} hrefFor={hrefFor} />
          </section>
        </div>
      </section>
    </MorphArrival>
  );
}
