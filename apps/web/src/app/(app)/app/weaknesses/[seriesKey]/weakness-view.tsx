import { AiDisclosure, type Segment, SegmentText } from "@ci/ui/coach";
import { CaretLeftIcon, Icon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import {
  countsText,
  dayText,
  KpiTrendGrid,
  type KpiTrendItem,
  needMoreText,
  shareRangeText,
  shareText,
  WeaknessRowKey,
  weaknessRateText,
  weaknessScaleMax,
} from "@ci/ui/profile";
import { stateStyle, WeaknessTimeline } from "@ci/ui/timeline";
import Link from "next/link";
import { MorphArrival } from "../../../../../components/shared-morph";
import type { ChangeNoteView } from "../../../../../server/coaching";
import type { CoachScope, WeaknessDetail } from "../../../../../server/profile";
import { SectionHead } from "../../_overview/section-head";
import { scopeQuery } from "../queries";
import { morphName } from "../weaknesses-view";
import { Exemplars } from "./exemplars";

// `/app/weaknesses/[seriesKey]` (M5b Task 7): one weakness in full. The header is the shared
// element the list row morphs into; below it, where the estimate stands, its state over time,
// the weekly series, the latest change note, example positions and a way to ask the coach.

/** Weekly points with fewer chances than this are hollow and off the line. */
export const WEEKLY_MIN_CHANCES = 20;

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
        <span>{shareText(max)}</span>
      </div>
    </div>
  );
}

/** Where the band median sits against the stored range; only a clear miss says lower or higher. */
export function bandRelation(rate: { low: number; high: number }, median: number): string {
  if (median < rate.low) return "lower than your whole likely range";
  if (median > rate.high) return "higher than your whole likely range";
  return "inside your likely range";
}

function Header({ detail, scope, askHref }: Props & { askHref: string }) {
  const early = !detail.evidence.sufficient;
  const state = early ? "Early read" : stateStyle[detail.state].label;
  return (
    <header className="flex flex-col gap-4">
      <Link
        href={`/app/weaknesses?${scopeQuery(scope)}`}
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
              data-state={early ? "early" : detail.state}
              className={
                early
                  ? "rounded-pill border border-dashed border-ink-faint px-2.5 py-0.5 font-medium"
                  : "rounded-pill bg-well px-2.5 py-0.5 font-medium text-ink"
              }
            >
              {state}
            </span>
            <span>{AREA_LABEL[detail.area]}</span>
          </p>
        </div>
        <Button asChild variant="outline" className="w-fit shrink-0">
          <Link href={askHref}>Ask your coach about this</Link>
        </Button>
      </div>
    </header>
  );
}

function Standing({ detail }: { detail: WeaknessDetail }) {
  const early = !detail.evidence.sufficient;
  const none = detail.evidence.chances <= 0;
  const median = detail.band.median;
  return (
    <section
      aria-label="Where it stands"
      data-surface="sheet"
      className="grid grid-cols-1 gap-x-10 gap-y-8 rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)] md:py-8"
    >
      <div className="flex flex-col gap-1">
        <span className="sr-only">
          {none
            ? `No estimate yet: ${countsText(detail.evidence)}.`
            : `${early ? "Early read. " : ""}${weaknessRateText(detail.rate, detail.evidence)}.`}
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
                ? "font-serif text-3xl leading-tight font-semibold text-ink-soft underline decoration-rule-strong decoration-dashed underline-offset-8"
                : "font-serif text-4xl leading-tight font-semibold text-ink"
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
          {countsText(detail.evidence)}
        </span>
        {early ? (
          <p className="mt-3 text-sm font-medium text-ink" style={LINING}>
            Early read: {needMoreText(detail.evidence.needMore)}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-4">
        {none ? null : (
          <>
            <IntervalScale rate={detail.rate} median={median} early={early} />
            <WeaknessRowKey className="text-xs" />
          </>
        )}
        <div className="flex flex-col gap-1 text-sm" style={LINING}>
          {median === null || !Number.isFinite(median) ? (
            <p className="text-ink-soft">No band reference yet</p>
          ) : (
            <>
              <p className="font-medium text-ink">Band median {shareText(median)}</p>
              <p className="text-ink-soft">
                {early || none
                  ? "Players in your rating band. A comparison waits for more chances."
                  : `Players in your rating band, ${bandRelation(detail.rate, median)}.`}
              </p>
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

function Weekly({ detail }: { detail: WeaknessDetail }) {
  const item = weeklyItem(detail);
  const first = item.points[0]?.week;
  const last = item.points.at(-1)?.week;
  if (!first || !last) {
    return <p className="text-base text-ink-soft">No weekly points yet.</p>;
  }
  return (
    <div data-surface="sheet" className="rounded-sheet bg-sheet px-(--space-panel-pad) py-6">
      <KpiTrendGrid
        items={[item]}
        range={{ from: first, to: last }}
        minN={WEEKLY_MIN_CHANCES}
        nUnit={CHANCES}
        headingLevel={3}
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

export function WeaknessView({
  detail,
  scope,
  notes,
  reviewIds,
  askHref,
}: Props & {
  notes: ChangeNoteView[];
  /** `games.id` by platform game id, for the exemplars and the note's moves. */
  reviewIds: Map<string, string>;
  askHref: string;
}) {
  const hrefFor = (s: Segment): string | null => {
    if (s.kind !== "move" && s.kind !== "best" && s.kind !== "pv") return null;
    const id = reviewIds.get(s.gameId);
    return id ? `/app/games/${encodeURIComponent(id)}?ply=${s.ply}` : null;
  };
  return (
    <MorphArrival
      name={morphName(detail.seriesKey)}
      className="flex flex-col gap-8"
      header={<Header detail={detail} scope={scope} askHref={askHref} />}
    >
      <Standing detail={detail} />

      <section aria-labelledby="timeline-heading" className="flex flex-col gap-5">
        <SectionHead id="timeline-heading" title="State over time" />
        <Timeline detail={detail} />
      </section>

      <div className="grid grid-cols-1 gap-x-10 gap-y-8 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <section aria-labelledby="weekly-heading" className="flex min-w-0 flex-col gap-5">
          <SectionHead id="weekly-heading" title="Week by week">
            <p className="text-sm text-ink-soft">
              The raw share of chances each week, with its 80% range. Weeks with fewer than{" "}
              {WEEKLY_MIN_CHANCES} chances are hollow and off the line.
            </p>
          </SectionHead>
          <Weekly detail={detail} />
        </section>

        <section aria-labelledby="note-heading" className="flex min-w-0 flex-col gap-5">
          <SectionHead id="note-heading" title="What changed">
            <p className="text-sm text-ink-soft">The coach's note on the latest detected change.</p>
          </SectionHead>
          <Note notes={notes} hrefFor={hrefFor} />
        </section>
      </div>

      <section aria-labelledby="examples-heading" className="flex flex-col gap-5">
        <SectionHead id="examples-heading" title="Example positions" />
        <Exemplars exemplars={detail.exemplars} reviewIds={reviewIds} />
      </section>
    </MorphArrival>
  );
}
