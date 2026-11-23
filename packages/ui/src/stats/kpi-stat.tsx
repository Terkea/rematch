"use client";

import { Scale } from "@visx/visx";
import { useEffect, useRef } from "react";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
import { type ExplainEntry, ExplainPopover } from "../profile/explain.js";
import {
  type BandInterval,
  bandText,
  basisText,
  bound,
  formatAt,
  KPI_MIN_GAMES,
  needText,
  type StatFormat,
  targetText,
} from "./band.js";
import { Sparkline } from "./sparkline.js";
import { Stat } from "./stat.js";

export interface KpiStatProps {
  label: string;
  value: number;
  format: StatFormat;
  /** The 80% interval from R2's statistics (Wilson for rates, bootstrap for accuracy). */
  interval: BandInterval;
  /** Below `KPI_MIN_GAMES` games the row shows no number: its basis and "Need N more games". */
  basis: { games: number; timeClass: string };
  /**
   * At least `KPI_MIN_GAMES` but below the metric's own minimum evidence: labelled "Early
   * estimate"; its band is as wide as it really is.
   */
  provisional?: boolean;
  /** One value per snapshot, oldest first; nulls are windows with no games. */
  trend?: (number | null)[];
  target?: { value: number; label: string };
  /** The level of the KPI's label heading (default 3). */
  headingLevel?: 2 | 3 | 4 | 5 | 6;
  /** Draw the trend in on mount (skipped under reduced motion). */
  drawIn?: boolean;
  /**
   * Draw the interval scale (default true). Off, the band is stated in words only: a host whose
   * KPIs have no meaningful reference range drops the ruler rather than imply one (Task 8).
   */
  scale?: boolean;
  /** The host states the basis once for the section: keep it for screen readers only. */
  basisHidden?: boolean;
  /** Write the trend's first and last values under the sparkline. */
  trendEnds?: boolean;
  /**
   * The games behind each trend point (weeks, aligned with `trend`). With it, the ends are named
   * ("First week 1.0, 2 games") and read out, so a one-game week can't pass for a rate.
   */
  trendGames?: number[];
  /** Which way is better for this KPI, written under the sparkline (never a verdict). */
  better?: "higher" | "lower";
  /** Roll the value up from zero on a first view (see `Stat`). */
  rollIn?: boolean;
  rollDelay?: number;
  className?: string;
  /** The glossary entry: its meaning under the label, and an "i" that opens the rest. */
  explain?: ExplainEntry | null;
}

/** The halo reads the host's surface, as `--eval-graph-surface` does, and is never set here. */
const HALO = "stroke-[var(--stat-surface,var(--color-sheet))]";

/** Scale ends: whole percents ("50%"), decimals to one place. */
const end = (v: number, format: StatFormat) =>
  format === "percent" ? formatAt(v, format, 0) : bound(v, format);

function trendLabel(label: string, trend: (number | null)[], format: StatFormat): string {
  const numbers = trend.filter((v): v is number => v !== null);
  const first = numbers[0];
  const last = numbers[numbers.length - 1];
  if (first === undefined || last === undefined || numbers.length < 2) return label;
  const gaps = trend.length - numbers.length;
  const gapText = gaps ? `, ${gaps} with no games` : "";
  return `${label} over ${trend.length} snapshots${gapText}: from ${bound(first, format)} to ${bound(last, format)}`;
}

/** Every percent scale spans exactly this much, so band widths compare across percent KPIs. */
const PERCENT_SPAN = 0.4;

const round3 = (v: number) => Math.round(v * 1000) / 1000;

/**
 * The scale's domain, labelled at both ends.
 *
 * Percent KPIs always get a 40-point window, never 0–100%: centred on the band, value and target
 * when they fit (start rounded to 10 points, widened back if rounding would cut them), otherwise
 * centred on the value, with the band running off the track. It is clamped inside 0–100%.
 *
 * Other formats run from zero (or below, for negatives) to a nice value about 15% past the band,
 * value and target, so a band is never zoomed into looking more precise than it is.
 */
export function domainOf(p: KpiStatProps): [number, number] {
  const ends = [p.interval.low, p.interval.high, p.value];
  if (p.target) ends.push(p.target.value);
  const lo = Math.min(...ends);
  const hi = Math.max(...ends);
  if (p.format === "percent") {
    const fits = hi - lo <= PERCENT_SPAN;
    const centre = fits ? (lo + hi) / 2 : p.value;
    let start = Math.round((centre - PERCENT_SPAN / 2) * 10) / 10;
    if (fits && (start > lo || start + PERCENT_SPAN < hi)) start = centre - PERCENT_SPAN / 2;
    start = Math.min(1 - PERCENT_SPAN, Math.max(0, start));
    return [round3(start), round3(start + PERCENT_SPAN)];
  }
  const a = Math.min(0, lo);
  const b = Math.max(0, hi);
  const pad = (b - a) * 0.15 || 1;
  const scale = Scale.scaleLinear({
    domain: [a < 0 ? a - pad : a, b > 0 ? b + pad : b],
    nice: true,
  });
  const [x = a, y = b] = scale.domain();
  return [x, y];
}

/** An open chevron past the track's end: the interval continues beyond the scale. */
function ContinuesCap({ side }: { side: "low" | "high" }) {
  const d = side === "high" ? "M2,7 L7,12 L2,17" : "M-2,7 L-7,12 L-2,17";
  return (
    <svg x={side === "high" ? "100%" : "0"} y="0" overflow="visible" aria-hidden="true">
      <path
        data-part={`band-continues-${side}`}
        d={d}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="stroke-rule-strong"
      />
    </svg>
  );
}

/** The interval scale: a `rule` track, the `rule-strong` band, the `ink` value tick and the target. */
function BandScale(props: KpiStatProps) {
  const [lo, hi] = domainOf(props);
  const at = (v: number) => ((v - lo) / (hi - lo)) * 100;
  const pct = (v: number) => `${at(v).toFixed(2)}%`;
  const { interval, value, target, format } = props;
  const reduced = useReducedMotion();

  // The number's clock: one progress value, on NumberFlow's own `dur.reveal` and `ease.pen`, carries
  // the tick from the shown value to the new one, so it travels with the roll. Reduced motion jumps.
  const x = useMotionValue(pct(value));
  const shown = useRef(value);
  const progress = useMotionValue(1);
  useEffect(() => {
    const from = shown.current;
    const place = (v: number) => {
      shown.current = v;
      x.set(`${(((v - lo) / (hi - lo)) * 100).toFixed(2)}%`);
    };
    if (reduced || from === value) {
      progress.jump(1);
      place(value);
      return;
    }
    progress.jump(0);
    const controls = animate(progress, 1, {
      duration: dur.reveal,
      ease: ease.pen,
      onUpdate: (p) => place(from + (value - from) * p),
    });
    return () => controls.stop();
  }, [value, reduced, lo, hi, progress, x]);
  // Under reduced motion the tick renders straight at the value, with no frame in between.
  const tickX = reduced ? pct(value) : x;

  // The band is clipped to the window; a clipped side gets an open "continues" cap, never a fade.
  const bandLo = Math.max(interval.low, lo);
  const bandHi = Math.min(interval.high, hi);
  const runsLow = interval.low < lo;
  const runsHigh = interval.high > hi;
  const targetShown = target !== undefined && target.value >= lo && target.value <= hi;

  return (
    <div aria-hidden="true" className="flex items-center gap-3 text-xs text-ink-faint">
      <span>{end(lo, format)}</span>
      <svg aria-hidden="true" className="block h-6 min-w-0 flex-1 overflow-visible">
        {/* Track: decorative, the whole domain. */}
        <rect x="0" y="11" width="100%" height="2" rx="1" className="fill-rule" />
        <rect
          data-part="band"
          x={pct(bandLo)}
          y="8"
          width={`${(at(bandHi) - at(bandLo)).toFixed(2)}%`}
          height="8"
          rx={runsLow || runsHigh ? 0 : 2}
          className="fill-rule-strong"
        />
        {runsLow && <ContinuesCap side="low" />}
        {runsHigh && <ContinuesCap side="high" />}
        {targetShown && (
          <svg x={pct(target.value)} y="0" overflow="visible" aria-hidden="true">
            {/* Target: a good tick with a caret, on a surface halo, so shape tells it from the value. */}
            <line x1="0" x2="0" y1="3" y2="21" strokeWidth="5" className={HALO} />
            <line
              data-part="target"
              x1="0"
              x2="0"
              y1="3"
              y2="21"
              strokeWidth="2"
              strokeDasharray="3 2"
              className="stroke-good"
            />
            <path d="M-4,0 L4,0 L0,4 Z" className="fill-good" />
          </svg>
        )}
        <motion.line
          x1={tickX}
          x2={tickX}
          y1="2"
          y2="22"
          strokeWidth="6"
          strokeLinecap="round"
          className={HALO}
        />
        <motion.line
          data-part="value"
          x1={tickX}
          x2={tickX}
          y1="2"
          y2="22"
          strokeWidth="2.5"
          strokeLinecap="round"
          className="stroke-ink"
        />
      </svg>
      <span>{end(hi, format)}</span>
    </div>
  );
}

/**
 * One KPI as a list row: its value (rolling), the credible interval drawn as a band and written out
 * in words, its basis, and its trend. Never a stat banner (B9). Uncertainty is shown, not hidden.
 */
export function KpiStat(props: KpiStatProps) {
  const {
    label,
    value,
    format,
    interval,
    basis,
    provisional,
    trend,
    target,
    headingLevel = 3,
    drawIn,
    scale = true,
    basisHidden = false,
    trendEnds = false,
    trendGames,
    better,
    rollIn,
    rollDelay,
    className,
    explain,
  } = props;
  const Heading = `h${headingLevel}` as const;
  if (basis.games < KPI_MIN_GAMES) {
    // Below the one minimum (ADR 0047 R3): no number, band or trend, only the basis and shortfall.
    return (
      <div
        data-kpi="insufficient"
        className={cn("flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1", className)}
      >
        <div className="flex flex-col gap-0.5">
          <span className="flex items-center gap-1">
            <Heading className="text-base font-medium text-ink">{label}</Heading>
            {explain ? <ExplainPopover entry={explain} /> : null}
          </span>
          {explain ? <p className="m-0 text-sm text-ink-soft">{explain.meaning}</p> : null}
          <p className="text-sm text-ink-faint">{basisText(basis)}</p>
        </div>
        <p className="font-serif text-xl text-ink-soft">{needText(basis.games)}</p>
      </div>
    );
  }
  // The trend's least span: twice the current 80% interval, so noise looks like noise.
  const minSpan = 2 * (interval.high - interval.low);
  // Wide rows. With the ruler: label, value, ruler and words, trend. Without it (web polish critic
  // I3), the label and the value share one baseline, the words sit under the value, and the trend
  // takes the right edge, so the row reads as one line rather than three floating columns.
  const wide = scale
    ? {
        grid: "@min-[40rem]:grid-cols-[minmax(9rem,1fr)_7rem_minmax(14rem,22rem)_8.5rem]", // ban-lint-allow TOK2: grid track template, no spacing token
        label: "@min-[40rem]:self-center",
        value: "",
        trend: "@min-[40rem]:col-start-4 @min-[40rem]:row-start-1 @min-[40rem]:self-center",
        band: "@min-[40rem]:col-start-3 @min-[40rem]:row-start-1 @min-[40rem]:self-center",
      }
    : {
        grid: "@min-[40rem]:grid-cols-[minmax(9rem,16rem)_minmax(0,1fr)_8.5rem] @min-[40rem]:items-baseline @min-[40rem]:gap-y-1.5", // ban-lint-allow TOK2: grid track template, no spacing token
        label: "@min-[40rem]:col-start-1 @min-[40rem]:row-start-1",
        value: "@min-[40rem]:col-start-2 @min-[40rem]:row-start-1",
        trend:
          "@min-[40rem]:col-start-3 @min-[40rem]:row-span-2 @min-[40rem]:row-start-1 @min-[40rem]:self-start",
        band: "@min-[40rem]:col-start-2 @min-[40rem]:row-start-2",
      };
  return (
    <div className={cn("@container", className)}>
      <div
        className={cn(
          "grid grid-cols-[minmax(0,1fr)_minmax(5rem,8.5rem)] items-end gap-x-4 gap-y-2 @min-[40rem]:gap-x-6", // ban-lint-allow TOK2: grid track template, no spacing token
          wide.grid,
        )}
      >
        <div
          className={cn(
            "col-span-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 @min-[40rem]:col-span-1 @min-[40rem]:flex-col @min-[40rem]:gap-0.5",
            wide.label,
          )}
        >
          <span className="flex items-center gap-1">
            <Heading className="text-base font-medium text-ink">{label}</Heading>
            {explain ? <ExplainPopover entry={explain} /> : null}
          </span>
          {explain ? <p className="m-0 text-sm text-ink-soft">{explain.meaning}</p> : null}
          <p className={basisHidden ? "sr-only" : "text-sm text-ink-faint"}>
            {basisText(basis)}
            {provisional && (
              <>
                <span className="sr-only">. </span>
                <span className="ml-2 inline-block rounded-pill bg-well px-2 text-sm text-ink-soft">
                  Early estimate
                </span>
              </>
            )}
          </p>
        </div>
        <Stat
          value={value}
          format={format}
          {...(rollIn ? { rollIn } : {})}
          {...(rollDelay !== undefined ? { rollDelay } : {})}
          className={cn("font-serif text-3xl leading-none font-semibold text-ink", wide.value)}
        />
        <div className={cn("col-start-2 row-start-2 self-end", wide.trend)}>
          {trend && (
            <Sparkline
              values={trend}
              label={trendLabel(`${label} trend`, trend, format)}
              minSpan={minSpan}
              {...(drawIn ? { drawIn } : {})}
            />
          )}
          {trend && (trendEnds || better) && (
            <TrendEnds
              trend={trend}
              format={format}
              ends={trendEnds}
              better={better}
              {...(trendGames ? { games: trendGames } : {})}
            />
          )}
        </div>
        <div className={cn("col-span-2 flex flex-col gap-1 @min-[40rem]:col-span-1", wide.band)}>
          {scale && <BandScale {...props} />}
          <p className="flex flex-wrap justify-between gap-x-3 text-sm text-ink-soft">
            <span>{bandText(interval, format)}</span>
            {target && (
              <span className="inline-flex items-center gap-1.5 text-ink-faint">
                <svg
                  aria-hidden="true"
                  width="8"
                  height="10"
                  viewBox="0 0 8 10"
                  className="shrink-0"
                >
                  <path d="M0,0 L8,0 L4,4 Z" className="fill-good" />
                  <path
                    d="M4,5 V10"
                    strokeWidth="2"
                    strokeDasharray="2 1.5"
                    className="stroke-good"
                  />
                </svg>
                <span>
                  {target.label} {targetText(target.value, interval, format)}
                </span>
              </span>
            )}
          </p>
        </div>
      </div>
    </div>
  );
}

/** The sparkline's first and last values, and which way is better: a convention, not a verdict. */
function TrendEnds({
  trend,
  format,
  ends,
  better,
  games,
}: {
  trend: (number | null)[];
  format: StatFormat;
  ends: boolean;
  better: "higher" | "lower" | undefined;
  games?: number[];
}) {
  const at = trend.flatMap((v, i) => (v === null ? [] : [i]));
  const firstAt = at[0];
  const lastAt = at[at.length - 1];
  const first = firstAt === undefined ? undefined : trend[firstAt];
  const last = lastAt === undefined ? undefined : trend[lastAt];
  const shown = ends && at.length >= 2 && first != null && last != null;
  const gamesText = (i: number | undefined) => {
    const n = i === undefined ? undefined : games?.[i];
    return n === undefined ? "" : ` (${n} ${n === 1 ? "game" : "games"})`;
  };
  return (
    <div data-part="trend-ends" className="mt-1 text-xs text-ink-soft">
      {shown &&
        (games ? (
          // One end per line; "week" is for screen readers, the section says "one point a week".
          <>
            <p className="whitespace-nowrap">
              First<span className="sr-only"> week</span> {bound(first, format)}
              {gamesText(firstAt)}
            </p>
            <p className="whitespace-nowrap">
              Latest<span className="sr-only"> week</span>{" "}
              <span className="font-medium text-ink">{bound(last, format)}</span>
              {gamesText(lastAt)}
            </p>
          </>
        ) : (
          <p aria-hidden="true" className="flex justify-between gap-2">
            <span>{bound(first, format)}</span>
            <span className="font-medium text-ink">{bound(last, format)}</span>
          </p>
        ))}
      {better && <p>{better === "lower" ? "Lower is better" : "Higher is better"}</p>}
    </div>
  );
}
