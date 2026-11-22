"use client";

import { useId } from "react";
import { cn } from "../lib/index.js";
import { Sparkline } from "../stats/sparkline.js";
import { stateStyle, type WeaknessState } from "../timeline/states.js";
import {
  bandComparisonText,
  dayText,
  needMoreText,
  shareRangeText,
  shareText,
  weaknessRateText,
} from "./text.js";

export interface WeaknessRowProps {
  label: string;
  state: WeaknessState;
  /** The stored posterior mean and its stored 80% interval, shares of chances (0 to 1). */
  rate: { mean: number; low: number; high: number };
  /** Raw counts (`k_unweighted` of `n_unweighted`) and whether they reach `min_evidence`. */
  evidence: { events: number; chances: number; sufficient: boolean; needMore: number | null };
  /** The rating band's median rate, or null when there is no band reference. */
  band: { median: number | null };
  href: string;
  /** Weekly rates, oldest first; null is a week with no chances (a gap, never a zero). */
  trend: (number | null)[];
  /** The detected onset (ISO date), or null when none was detected. */
  onset: string | null;
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
 * shape). The strip's scale starts at zero and runs past the larger of the high end and the
 * median, so a wider interval always looks wider. Words beside it carry every number.
 */
function RateStrip({ rate, median }: { rate: WeaknessRowProps["rate"]; median: number | null }) {
  const top = Math.max(rate.high, median ?? 0);
  const max = top > 0 ? top * 1.25 : 1;
  const at = (v: number) => `${Math.min(100, Math.max(0, (v / max) * 100)).toFixed(3)}%`;
  return (
    <div aria-hidden="true" data-part="rate-strip" className="relative h-5 w-full">
      <div className="absolute inset-x-0 top-2 h-px bg-rule" />
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
  );
}

/**
 * One weakness as a ruled list row, linked to its detail: the label and state (pattern and colour,
 * named in words), the estimated rate with its stored 80% range and raw counts, where that range
 * sits against the band median, and the weekly trend. Below `min_evidence` the row is an early
 * read: the state chip says "Early read" and the band comparison gives way to how many more
 * chances are needed (ADR 0068 R3, ADR 0072).
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
  className,
}: WeaknessRowProps) {
  const early = !evidence.sufficient;
  const chip = early ? "Early read" : stateStyle[state].label;
  const counts = `${new Intl.NumberFormat("en-GB").format(evidence.events)} of ${new Intl.NumberFormat("en-GB").format(evidence.chances)} chances`;
  const measured = trend.filter((v) => v !== null).length;
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
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-4 gap-y-3 px-3 py-3.5 @min-[720px]:grid-cols-[minmax(11rem,1fr)_9rem_minmax(12rem,1.2fr)_8rem] @min-[720px]:items-center @min-[720px]:gap-x-6">
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
                Onset detected <time dateTime={onset.slice(0, 10)}>{dayText(onset)}</time>
              </span>
            ) : null}
          </div>
        </div>

        <div className="flex flex-col items-end text-right @min-[720px]:items-start @min-[720px]:text-left">
          <span className="sr-only">{weaknessRateText(rate, evidence)}.</span>
          <span
            aria-hidden="true"
            className={cn(
              "font-serif leading-none font-semibold",
              early ? "text-xl text-ink-soft" : "text-2xl text-ink",
            )}
            style={NUMERALS}
          >
            {shareText(rate.mean)}
          </span>
          <span aria-hidden="true" className="mt-1 text-sm text-ink-soft" style={LINING}>
            {shareRangeText(rate.low, rate.high)}
          </span>
          <span aria-hidden="true" className="text-sm text-ink-faint" style={LINING}>
            {counts}
          </span>
        </div>

        <div className="col-span-2 flex flex-col gap-1 @min-[720px]:col-span-1">
          {early ? (
            <>
              <span className="text-sm font-medium text-ink">Early read</span>
              <span className="text-sm text-ink-soft" style={LINING}>
                {needMoreText(evidence.needMore)}
              </span>
            </>
          ) : (
            <>
              <RateStrip rate={rate} median={band.median} />
              <span className="text-sm text-ink-soft" style={LINING}>
                {bandComparisonText(rate, band.median)}
              </span>
            </>
          )}
        </div>

        <div className="col-span-2 flex flex-col gap-0.5 @min-[720px]:col-span-1">
          <span aria-hidden="true" className="text-xs text-ink-faint">
            Weekly rate
          </span>
          <Sparkline
            values={trend}
            label={`${label}, weekly rate over ${measured} measured week${measured === 1 ? "" : "s"}`}
          />
        </div>
      </div>
    </div>
  );
}
