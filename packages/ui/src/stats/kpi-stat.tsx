"use client";

import { Scale } from "@visx/visx";
import { useEffect, useRef } from "react";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
import { Sparkline } from "./sparkline.js";
import { DEFAULT_LOCALE, Stat, type StatFormat } from "./stat.js";

export interface KpiStatProps {
  label: string;
  value: number;
  format: StatFormat;
  /** The credible interval from R2's statistics (Wilson for rates, bootstrap for accuracy). */
  interval: { low: number; high: number; level: 0.8 | 0.9 };
  basis: { games: number; timeClass: string };
  /** Below the minimum evidence: labelled "Early estimate"; its band is as wide as it really is. */
  provisional?: boolean;
  /** One value per snapshot, oldest first; nulls are windows with no games. */
  trend?: (number | null)[];
  target?: { value: number; label: string };
  /** The level of the KPI's label heading (default 3). */
  headingLevel?: 2 | 3 | 4 | 5 | 6;
  /** Draw the trend in on mount (skipped under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

type Interval = KpiStatProps["interval"];

/** The halo reads the host's surface, as `--eval-graph-surface` does, and is never set here. */
const HALO = "stroke-[var(--stat-surface,var(--color-sheet))]";

/**
 * Interval ends: one step coarser than the value for decimals (0.1 against 0.01), and the value's
 * own step for percent (0.1%) and integer (1). `digits` is the fraction digits shown.
 */
const STEP: Record<StatFormat, { factor: number; digits: number }> = {
  percent: { factor: 1000, digits: 1 },
  decimal: { factor: 10, digits: 1 },
  integer: { factor: 1, digits: 0 },
};

const formatAt = (v: number, format: StatFormat, digits: number) =>
  new Intl.NumberFormat(DEFAULT_LOCALE, {
    ...(format === "percent" ? { style: "percent" } : {}),
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(v);

const bound = (v: number, format: StatFormat) => formatAt(v, format, STEP[format].digits);

/**
 * The interval's ends rounded outward at the shown precision: low floored, high ceiled, so the
 * stated range is never narrower than the computed one. The epsilon absorbs float noise
 * (0.766 × 1000 is 766.0000000000001, which must not ceil to 767).
 */
export function bandEnds(interval: Interval, format: StatFormat): [number, number] {
  const f = STEP[format].factor;
  return [Math.floor(interval.low * f + 1e-9) / f, Math.ceil(interval.high * f - 1e-9) / f];
}

/** Scale ends: whole percents ("50%"), decimals to one place. */
const end = (v: number, format: StatFormat) =>
  format === "percent" ? formatAt(v, format, 0) : bound(v, format);

/** "Based on 38 rapid games" (one game is singular). */
export const basisText = ({ games, timeClass }: KpiStatProps["basis"]) =>
  `Based on ${games} ${timeClass} game${games === 1 ? "" : "s"}`;

/** "80% likely between 61.0 and 67.5", with the ends rounded outward. */
export const bandText = (interval: Interval, format: StatFormat) => {
  const [low, high] = bandEnds(interval, format);
  return `${Math.round(interval.level * 100)}% likely between ${bound(low, format)} and ${bound(high, format)}`;
};

/** The target at the ends' precision, or one place finer when it would read the same as an end. */
export function targetText(target: number, interval: Interval, format: StatFormat): string {
  const ends = bandEnds(interval, format).map((v) => bound(v, format));
  const text = bound(target, format);
  return ends.includes(text) ? formatAt(target, format, STEP[format].digits + 1) : text;
}

function trendLabel(label: string, trend: (number | null)[], format: StatFormat): string {
  const numbers = trend.filter((v): v is number => v !== null);
  const first = numbers[0];
  const last = numbers[numbers.length - 1];
  if (first === undefined || last === undefined || numbers.length < 2) return label;
  const gaps = trend.length - numbers.length;
  const gapText = gaps ? `, ${gaps} with no games` : "";
  return `${label} over ${trend.length} snapshots${gapText}: from ${bound(first, format)} to ${bound(last, format)}`;
}

/** z for a two-sided 80% and 90% interval: a 90% interval is 1.6449 / 1.2816 times an 80% one. */
const TO_80: Record<Interval["level"], number> = { 0.8: 1, 0.9: 1.2816 / 1.6449 };

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
    className,
  } = props;
  const Heading = `h${headingLevel}` as const;
  // The trend's least span: twice the current 80% interval, so noise looks like noise.
  const minSpan = 2 * (interval.high - interval.low) * TO_80[interval.level];
  return (
    <div className={cn("@container", className)}>
      <div
        className="grid grid-cols-[minmax(0,1fr)_minmax(5rem,7.5rem)] items-end gap-x-4 gap-y-2 @min-[40rem]:grid-cols-[minmax(9rem,1fr)_7rem_minmax(14rem,22rem)_7.5rem] @min-[40rem]:gap-x-6" /* ban-lint-allow TOK2: grid track template, no spacing token */
      >
        <div className="col-span-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 @min-[40rem]:col-span-1 @min-[40rem]:flex-col @min-[40rem]:gap-0.5 @min-[40rem]:self-center">
          <Heading className="text-base font-medium text-ink">{label}</Heading>
          <p className="text-sm text-ink-faint">
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
          className="font-serif text-3xl leading-none font-semibold text-ink"
        />
        <div className="col-start-2 row-start-2 self-end @min-[40rem]:col-start-4 @min-[40rem]:row-start-1 @min-[40rem]:self-center">
          {trend && (
            <Sparkline
              values={trend}
              label={trendLabel(`${label} trend`, trend, format)}
              minSpan={minSpan}
              {...(drawIn ? { drawIn } : {})}
            />
          )}
        </div>
        <div className="col-span-2 flex flex-col gap-1 @min-[40rem]:col-span-1 @min-[40rem]:col-start-3 @min-[40rem]:row-start-1 @min-[40rem]:self-center">
          <BandScale {...props} />
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
