"use client";

import { useId } from "react";
import { cn } from "../lib/index.js";
import { Sparkline } from "../stats/sparkline.js";
import { stateStyle, type WeaknessState } from "../timeline/states.js";
import {
  bandComparisonText,
  countsText,
  DEFAULT_BAND_LABEL,
  DEFAULT_COUNTS,
  dayText,
  needMoreText,
  shareRangeText,
  shareText,
  trendPendingText,
  type WeaknessCounts,
  weaknessRateText,
  weaknessScaleMax,
} from "./text.js";

export interface WeaknessRowProps {
  label: string;
  state: WeaknessState;
  /** The stored posterior mean and its stored 80% interval, shares of chances (0 to 1). */
  rate: { mean: number; low: number; high: number };
  /** Raw counts (`k_unweighted` of `n_unweighted`) and whether they reach `min_evidence`. */
  evidence: { events: number; chances: number; sufficient: boolean; needMore: number | null };
  /**
   * The rating band's median rate, or null when there is no band reference, and the band's name
   * ("Players rated 1400–1600"; default "Players in your rating band").
   */
  band: { median: number | null; label?: string };
  href: string;
  /** Weekly rates, oldest first; null is a week with no chances (a gap, never a zero). */
  trend: (number | null)[];
  /** The detected onset (ISO date), or null when none was detected. */
  onset: string | null;
  /** What the counts are, as plural nouns (default "times" in "chances"). */
  counts?: WeaknessCounts;
  /** The top of the interval scale, shared down a list (`weaknessScaleMax`); default this row's. */
  scaleMax?: number;
  className?: string;
}

const NUMERALS = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
/** Sans text with figures: lining but proportional, so stops and commas keep their own width. */
const LINING = { fontVariantNumeric: "lining-nums" } as const;
const fin = Number.isFinite;

/**
 * The state's own mark at chip size: the timeline's colour, pattern and bar weight (`stateStyle`),
 * so the chip and the swimlane read as one code. An early read is a dashed empty box: no state yet.
 */
function StateMark({ state }: { state: WeaknessState | "early" }) {
  const id = `wr${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  if (state === "early") {
    return (
      <svg aria-hidden="true" width={16} height={12} className="shrink-0">
        <rect
          x={0.75}
          y={0.75}
          width={14.5}
          height={10.5}
          rx={1.5}
          fill="none"
          strokeWidth={1.5}
          strokeDasharray="2.5 2"
          className="stroke-ink-faint"
        />
      </svg>
    );
  }
  const s = stateStyle[state];
  const bar = { full: { y: 2, h: 8 }, band: { y: 4, h: 4 }, hairline: { y: 5.25, h: 1.5 } }[
    s.weight
  ];
  const paint = s.pattern === "none" ? s.fill : `url(#${id})`;
  return (
    <svg aria-hidden="true" width={16} height={12} className="shrink-0">
      <defs>
        {s.pattern === "dots" ? (
          <pattern id={id} width={3.5} height={3.5} patternUnits="userSpaceOnUse">
            <circle cx={1.75} cy={1.75} r={1} style={{ fill: s.fill }} />
          </pattern>
        ) : null}
        {s.pattern === "hatch" ? (
          <pattern
            id={id}
            width={4}
            height={4}
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <rect x={0} y={0} width={1.6} height={4} style={{ fill: s.fill }} />
            {s.angle === "cross" ? (
              <rect x={0} y={0} width={4} height={1.6} style={{ fill: s.fill }} />
            ) : null}
          </pattern>
        ) : null}
      </defs>
      <rect x={0} y={0} width={16} height={12} rx={1.5} className="fill-well" />
      <rect x={1.5} y={bar.y} width={13} height={bar.h} style={{ fill: paint }} />
    </svg>
  );
}

/**
 * The stored interval on a small strip: a `rule-strong` band from low to high, the mean as an
 * `ink` tick, and the band median as a short `pencil` tick hanging below (AreaScore's median
 * shape). The strip runs from zero to `max`, which a list shares across its rows
 * (`weaknessScaleMax`), so a wider or higher interval always looks wider or higher; both ends
 * are labelled. Words beside it carry every number.
 */
function RateStrip({
  rate,
  median,
  max,
}: {
  rate: WeaknessRowProps["rate"];
  median: number | null;
  max: number;
}) {
  const at = (v: number) => `${Math.min(100, Math.max(0, (v / max) * 100)).toFixed(3)}%`;
  return (
    <div aria-hidden="true" data-part="rate-strip" className="flex flex-col gap-0.5">
      <div className="relative h-5 w-full">
        <div className="absolute inset-x-0 top-2 h-px bg-rule-strong" />
        <div className="absolute top-1 left-0 h-2 w-px bg-rule-strong" />
        <div className="absolute top-1 right-0 h-2 w-px bg-rule-strong" />
        <div
          data-part="interval"
          className="absolute top-1 h-2 rounded-(--radius-board) bg-rule-strong"
          style={{ left: at(rate.low), width: `calc(${at(rate.high)} - ${at(rate.low)})` }}
        />
        <div
          data-part="mean"
          className="absolute top-0 h-4 w-0.75 -translate-x-1/2 rounded-full bg-ink outline-2 outline-(--row-surface,var(--color-sheet))"
          style={{ left: at(rate.mean) }}
        />
        {median !== null && fin(median) ? (
          <div
            data-part="median"
            className="absolute top-3 h-2.5 w-0.5 -translate-x-1/2 rounded-full bg-pencil"
            style={{ left: at(median) }}
          />
        ) : null}
      </div>
      <div className="flex justify-between text-xs text-ink-faint" style={LINING}>
        <span>0%</span>
        <span data-part="scale-max">{shareText(max)}</span>
      </div>
    </div>
  );
}

/** The key for a list of rows: the strip's three marks, drawn with their own shapes. */
export function WeaknessRowKey({
  bandLabel = DEFAULT_BAND_LABEL,
  className,
}: {
  bandLabel?: string;
  className?: string;
}) {
  return (
    <ul
      aria-label="Key"
      className={cn(
        "m-0 flex list-none flex-wrap gap-x-5 gap-y-1 p-0 text-sm text-ink-soft",
        className,
      )}
    >
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="h-4 w-0.75 rounded-full bg-ink" />
        Estimated rate
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="h-2 w-5 rounded-(--radius-board) bg-rule-strong" />
        80% range
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="mt-2 h-2.5 w-0.5 rounded-full bg-pencil" />
        {bandLabel}, median
      </li>
    </ul>
  );
}

/**
 * One weakness as a ruled list row, linked to its detail: the label and state (pattern and colour,
 * named in words), the estimated rate labelled as such with its stored 80% range and raw counts
 * (named by `counts`), where that range sits against the rating band's median on a scale shared
 * down the list (`scaleMax`), and the weekly trend. Below `min_evidence` the row is an early read:
 * the chip says "Early read", the estimate is shown as provisional (muted, dashed underline,
 * "Early estimate"), and the strip, comparison and trend give way to how many more chances are
 * needed. With no chances at all there is no estimate to show (ADR 0068 R3, ADR 0072).
 *
 * The label is the link and stretches over the row, so the whole row is one 44px+ target and the
 * reading order stays label, state, rate, comparison, trend.
 */
export function WeaknessRow({
  label,
  state,
  rate,
  evidence,
  band,
  href,
  trend,
  onset,
  counts = DEFAULT_COUNTS,
  scaleMax,
  className,
}: WeaknessRowProps) {
  const early = !evidence.sufficient;
  const none = evidence.chances <= 0;
  const chip = early ? "Early read" : stateStyle[state].label;
  const measured = trend.filter((v) => v !== null && fin(v)).length;
  const max =
    scaleMax !== undefined && scaleMax > 0 ? scaleMax : weaknessScaleMax([{ rate, band }]);
  return (
    <div
      data-weakness-row
      data-early={early || undefined}
      className={cn(
        "@container group relative rounded-field transition-colors duration-(--dur-quick) ease-out hover:bg-(--surface-hover)",
        className,
      )}
    >
      {/* ban-lint-allow TOK2: grid track template, no spacing token */}
      <div className="grid grid-cols-1 items-start gap-x-4 gap-y-3 px-3 py-3.5 @min-[720px]:grid-cols-[minmax(11rem,1fr)_13rem_minmax(12rem,1.2fr)_9rem] @min-[720px]:items-center @min-[720px]:gap-x-6">
        <div className="flex min-w-0 flex-col gap-1.5">
          <a
            href={href}
            className="font-medium text-ink outline-none before:absolute before:inset-0 before:rounded-field focus-visible:before:outline-2 focus-visible:before:outline-offset-2 focus-visible:before:outline-ink"
          >
            {label}
          </a>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
            <span className="inline-flex items-center gap-1.5 text-ink-soft">
              <StateMark state={early ? "early" : state} />
              <span>{chip}</span>
            </span>
            {onset !== null && !early ? (
              <span className="text-ink-faint">
                Onset detected{" "}
                <time dateTime={onset.slice(0, 10)} className="whitespace-nowrap">
                  {dayText(onset)}
                </time>
              </span>
            ) : null}
          </div>
        </div>

        <div className="flex flex-col">
          <span className="sr-only">
            {none
              ? `No estimate yet: ${countsText(evidence, counts)}.`
              : `${early ? "Early read. " : ""}${weaknessRateText(rate, evidence)}.`}
          </span>
          {none ? (
            <span aria-hidden="true" className="text-sm font-medium text-ink-soft">
              No chances yet
            </span>
          ) : (
            <>
              <span aria-hidden="true" className="text-xs text-ink-soft">
                {early ? "Early estimate" : "Estimated"} % of {counts.chance}
              </span>
              <span
                aria-hidden="true"
                className={cn(
                  "font-serif leading-tight font-semibold",
                  early
                    ? "text-lg text-ink-soft underline decoration-rule-strong decoration-dashed underline-offset-4"
                    : "text-2xl text-ink",
                )}
                style={NUMERALS}
              >
                {shareText(rate.mean)}
              </span>
              <span aria-hidden="true" className="text-sm text-ink-soft" style={LINING}>
                80% range {shareRangeText(rate.low, rate.high)}
              </span>
            </>
          )}
          <span aria-hidden="true" className="text-sm text-ink-faint" style={LINING}>
            {countsText(evidence, counts)}
          </span>
        </div>

        <div className="flex flex-col gap-1">
          {early ? (
            <>
              <span className="text-sm font-medium text-ink">Early read</span>
              <span className="text-sm text-ink-soft" style={LINING}>
                {needMoreText(evidence.needMore, counts)}
              </span>
            </>
          ) : (
            <>
              <RateStrip rate={rate} median={band.median} max={max} />
              <span className="text-sm text-ink-soft" style={LINING}>
                {bandComparisonText(rate, band.median, band.label)}
              </span>
            </>
          )}
        </div>

        <div className="flex flex-col gap-0.5">
          {early || measured < 2 ? (
            <span className="text-xs text-ink-soft">
              {early
                ? trendPendingText(evidence.needMore, counts)
                : "Weekly trend after more weeks"}
            </span>
          ) : (
            <>
              <span aria-hidden="true" className="text-xs text-ink-soft">
                Weekly rate
              </span>
              <Sparkline
                values={trend}
                label={`${label}, weekly rate over ${measured} measured weeks`}
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
