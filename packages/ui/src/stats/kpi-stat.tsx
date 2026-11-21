"use client";

import { Scale } from "@visx/visx";
import { cn } from "../lib/index.js";
import { Sparkline } from "./sparkline.js";
import { DEFAULT_LOCALE, Stat, type StatFormat } from "./stat.js";

export interface KpiStatProps {
  label: string;
  value: number;
  format: StatFormat;
  /** The credible interval from R2's statistics (Wilson for rates, bootstrap for accuracy). */
  interval: { low: number; high: number; level: 0.8 | 0.9 };
  basis: { games: number; timeClass: string };
  /** Below the minimum evidence: the band is wide and the value is labelled "Early estimate". */
  provisional?: boolean;
  /** One value per snapshot, oldest first; nulls are windows with no games. */
  trend?: (number | null)[];
  target?: { value: number; label: string };
  /** Draw the trend in on mount (skipped under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

/**
 * Interval ends and targets: one step coarser than the value for decimals, so a band never reads
 * as more precise than the estimate behind it ("61.0 and 67.5").
 */
const BOUND: Record<StatFormat, Intl.NumberFormatOptions> = {
  percent: { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 },
  decimal: { minimumFractionDigits: 1, maximumFractionDigits: 1 },
  integer: { maximumFractionDigits: 0 },
};
const bound = (v: number, format: StatFormat) =>
  new Intl.NumberFormat(DEFAULT_LOCALE, BOUND[format]).format(v);

/** Scale ends: whole percents ("50%"), decimals to one place. */
const end = (v: number, format: StatFormat) =>
  format === "percent"
    ? new Intl.NumberFormat(DEFAULT_LOCALE, { style: "percent" }).format(v)
    : bound(v, format);

/** "Based on 38 rapid games" (one game is singular). */
export const basisText = ({ games, timeClass }: KpiStatProps["basis"]) =>
  `Based on ${games} ${timeClass} game${games === 1 ? "" : "s"}`;

/** "80% likely between 61.0 and 67.5". */
export const bandText = (interval: KpiStatProps["interval"], format: StatFormat) =>
  `${Math.round(interval.level * 100)}% likely between ${bound(interval.low, format)} and ${bound(interval.high, format)}`;

function trendLabel(label: string, trend: (number | null)[], format: StatFormat): string {
  const numbers = trend.filter((v): v is number => v !== null);
  const first = numbers[0];
  const last = numbers[numbers.length - 1];
  if (first === undefined || last === undefined || numbers.length < 2) return label;
  const gaps = trend.length - numbers.length;
  const gapText = gaps ? `, ${gaps} with no games` : "";
  return `${label} over ${trend.length} snapshots${gapText}: from ${bound(first, format)} to ${bound(last, format)}`;
}

/** Every percent scale spans this much, so band widths compare honestly across percent KPIs. */
const PERCENT_SPAN = 0.4;

/**
 * The scale's domain, labelled at both ends. Percent KPIs share one 40-point window (placed around
 * the band and target, inside 0–100%), so a band's width means the same on every row. Other formats
 * run from zero to a nice value past the band and target, so a band is never zoomed into looking
 * more precise than it is.
 */
function domainOf(p: KpiStatProps): [number, number] {
  if (p.format === "percent") {
    const lo = Math.min(p.interval.low, p.value, p.target?.value ?? 1);
    const hi = Math.max(p.interval.high, p.value, p.target?.value ?? 0);
    if (hi - lo > PERCENT_SPAN - 0.1) return [0, 1];
    const start = Math.round(((lo + hi) / 2 - PERCENT_SPAN / 2) * 10) / 10;
    const clamped = Math.min(1 - PERCENT_SPAN, Math.max(0, start));
    return [clamped, clamped + PERCENT_SPAN];
  }
  const hi = Math.max(p.interval.high, p.value, p.target?.value ?? 0);
  const lo = Math.min(0, p.interval.low, p.value, p.target?.value ?? 0);
  const scale = Scale.scaleLinear({ domain: [lo, hi * 1.15 || 1], nice: true });
  const [a = lo, b = hi] = scale.domain();
  return [a, b];
}

/** The interval scale: a `rule` track, the `rule-strong` band, the `ink` value tick and the target. */
function BandScale(props: KpiStatProps) {
  const [lo, hi] = domainOf(props);
  const pct = (v: number) => `${(((v - lo) / (hi - lo)) * 100).toFixed(2)}%`;
  const width = (a: number, b: number) => `${(((b - a) / (hi - lo)) * 100).toFixed(2)}%`;
  const { interval, value, target, format } = props;
  return (
    <div aria-hidden="true" className="flex items-center gap-2 text-xs text-ink-faint">
      <span>{end(lo, format)}</span>
      <svg aria-hidden="true" className="block h-6 min-w-0 flex-1 overflow-visible">
        {/* Track: decorative, the whole domain. */}
        <rect x="0" y="11" width="100%" height="2" rx="1" className="fill-rule" />
        <rect
          data-part="band"
          x={pct(interval.low)}
          y="8"
          width={width(interval.low, interval.high)}
          height="8"
          rx="2"
          className={cn("fill-rule-strong", props.provisional && "opacity-70")}
        />
        {target && (
          <svg x={pct(target.value)} y="0" overflow="visible" aria-hidden="true">
            {/* Target: a good tick with a caret, on a surface halo, so shape tells it from the value. */}
            <line
              x1="0"
              x2="0"
              y1="3"
              y2="21"
              strokeWidth="5"
              className="stroke-(--stat-surface)"
            />
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
        <line
          x1={pct(value)}
          x2={pct(value)}
          y1="2"
          y2="22"
          strokeWidth="6"
          strokeLinecap="round"
          className="stroke-(--stat-surface)"
        />
        <line
          data-part="value"
          x1={pct(value)}
          x2={pct(value)}
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
  const { label, value, format, interval, basis, provisional, trend, target, drawIn, className } =
    props;
  return (
    <div
      className={cn(
        "@container [--stat-surface:var(--color-sheet)]",
        "[[data-surface=paper]_&]:[--stat-surface:var(--color-paper)]",
        className,
      )}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(5rem,7.5rem)] items-end gap-x-4 gap-y-2 @min-[40rem]:grid-cols-[minmax(9rem,1fr)_7rem_minmax(14rem,22rem)_7.5rem] @min-[40rem]:gap-x-6">
        <div className="col-span-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 @min-[40rem]:col-span-1 @min-[40rem]:flex-col @min-[40rem]:gap-0.5 @min-[40rem]:self-center">
          <h3 className="text-base font-medium text-ink">{label}</h3>
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
                  {target.label} {bound(target.value, format)}
                </span>
              </span>
            )}
          </p>
        </div>
      </div>
    </div>
  );
}
