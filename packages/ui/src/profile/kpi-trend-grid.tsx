"use client";

import { cn } from "../lib/index.js";
import {
  dayText,
  formatKpiValue,
  isoDay,
  type KpiTrendItem,
  type KpiTrendPoint,
  kpiSummary,
  nText,
  onLine,
  shortDayText,
  weekText,
} from "./text.js";

export type { KpiTrendItem, KpiTrendPoint } from "./text.js";

export interface KpiTrendGridProps {
  items: KpiTrendItem[];
  /** One time axis for every multiple: the first and last week shown (ISO dates). */
  range: { from: string; to: string };
  /** Report dates, drawn as vertical `pencil` rules and listed under the grid. */
  events?: { date: string; label: string }[];
  /** Points with fewer games than this are hollow and off the line. */
  minN: number;
  className?: string;
}

/** The plot's drawing box. The SVG stretches to its column; strokes and dots never scale. */
const W = 300;
const H = 96;
const INSET = 8;
const NUMERALS = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
const LINING = { fontVariantNumeric: "lining-nums" } as const;
const fin = Number.isFinite;

const finite = (vs: (number | null)[]) => vs.filter((v): v is number => v !== null && fin(v));

/**
 * The value axis for one item: the extent of its values and stored band ends, padded by an eighth
 * of the span (or a tenth of the value when flat), clamped to the unit's own limits, top to bottom
 * in drawing units. Exported so tests can check the band lands exactly on `low` and `high`.
 */
export function kpiDomain(item: KpiTrendItem): [number, number] {
  const vs = finite(item.points.flatMap((p) => [p.value, p.low, p.high]));
  if (vs.length === 0) return [0, 1];
  let lo = Math.min(...vs);
  let hi = Math.max(...vs);
  const span = hi - lo;
  const pad = span > 0 ? span / 8 : Math.abs(hi) / 10 || 1;
  lo -= pad;
  hi += pad;
  const cap = item.unit === "rate" ? 1 : item.unit === "pct" ? 100 : Number.POSITIVE_INFINITY;
  if (item.unit !== null || vs.every((v) => v >= 0)) lo = Math.max(0, lo);
  hi = Math.min(cap, hi);
  return [lo, hi];
}

export function kpiYScale(item: KpiTrendItem): (v: number) => number {
  const [lo, hi] = kpiDomain(item);
  const span = hi - lo || 1;
  return (v: number) => INSET + (1 - (v - lo) / span) * (H - 2 * INSET);
}

/** Where a date sits on the shared axis, in drawing units. */
function xScale(range: KpiTrendGridProps["range"]): (iso: string) => number {
  const from = isoDay(range.from);
  const span = isoDay(range.to) - from;
  return (iso: string) => {
    const f = span > 0 ? (isoDay(iso) - from) / span : 0.5;
    return INSET + Math.min(1, Math.max(0, f)) * (W - 2 * INSET);
  };
}

const n2 = (v: number) => Number(v.toFixed(3));

/** Consecutive runs of points that pass `keep`, in order. */
function runsOf<T>(xs: T[], keep: (x: T) => boolean): T[][] {
  const out: T[][] = [];
  let cur: T[] = [];
  for (const x of xs) {
    if (keep(x)) cur.push(x);
    else if (cur.length) {
      out.push(cur);
      cur = [];
    }
  }
  if (cur.length) out.push(cur);
  return out;
}

/**
 * The stored band as one closed area per run: the high edge left to right, the low edge back. The
 * coordinates are not rounded, so the edges sit exactly on `low` and `high`.
 */
function bandPath(run: KpiTrendPoint[], x: (iso: string) => number, y: (v: number) => number) {
  const hi = run.map((p) => [x(p.week), y(p.high ?? 0)] as const);
  const lo = run.map((p) => [x(p.week), y(p.low ?? 0)] as const).reverse();
  if (run.length === 1) {
    const [[cx = 0, yh = 0] = []] = hi;
    const [[, yl = 0] = []] = lo;
    return `M${cx - 2.5},${yh}L${cx + 2.5},${yh}L${cx + 2.5},${yl}L${cx - 2.5},${yl}Z`;
  }
  const pts = [...hi, ...lo];
  return `${pts.map(([px, py], i) => `${i === 0 ? "M" : "L"}${px},${py}`).join("")}Z`;
}

const dot = (px: number, py: number) => `M${n2(px)},${n2(py)}h0`;

/** The headline of a multiple: the latest week on the line, else the latest measured week. */
function latest(item: KpiTrendItem, minN: number) {
  const pts = [...item.points].reverse();
  return (
    pts.find((p) => onLine(p, minN)) ?? pts.find((p) => p.value !== null && fin(p.value)) ?? null
  );
}

function Multiple({
  item,
  range,
  events,
  minN,
}: {
  item: KpiTrendItem;
  range: KpiTrendGridProps["range"];
  events: NonNullable<KpiTrendGridProps["events"]>;
  minN: number;
}) {
  const x = xScale(range);
  const y = kpiYScale(item);
  const [lo, hi] = kpiDomain(item);
  const head = latest(item, minN);
  const hasValues = head !== null;
  const unit = item.nUnit ?? { one: "game", many: "games" };
  const bands = runsOf(
    item.points,
    (p) => p.low !== null && p.high !== null && fin(p.low) && fin(p.high),
  );
  const lines = runsOf(item.points, (p) => onLine(p, minN));
  const hollow = item.points.filter((p) => p.value !== null && fin(p.value) && p.n < minN);
  const last = lines.at(-1)?.at(-1);
  const pc = (v: number) => `${((v / W) * 100).toFixed(3)}%`;

  return (
    <figure data-kpi={item.metric} className="m-0 flex min-w-0 flex-col gap-2">
      <figcaption className="flex flex-col gap-0.5">
        <h3 className="m-0 text-base font-medium text-ink">{item.label}</h3>
        {head ? (
          <p aria-hidden="true" className="m-0 flex flex-wrap items-baseline gap-x-2 text-sm">
            <span className="font-serif text-xl font-semibold text-ink" style={NUMERALS}>
              {formatKpiValue(head.value ?? Number.NaN, item.unit)}
            </span>
            <span className="text-ink-faint" style={LINING}>
              {weekText(head.week).replace(/^w/, "W")}, {nText(head.n, unit)}
              {head.n < minN ? `, fewer than ${minN}` : ""}
            </span>
          </p>
        ) : null}
        <span className="sr-only">{kpiSummary(item, minN)}</span>
      </figcaption>

      {hasValues ? (
        <div aria-hidden="true" className="flex gap-2">
          <div
            className="flex w-10 shrink-0 flex-col justify-between py-1 text-right text-xs text-ink-faint"
            style={LINING}
          >
            <span>{formatKpiValue(hi, item.unit)}</span>
            <span>{formatKpiValue(lo, item.unit)}</span>
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <div className="relative h-24 rounded-field bg-well [--row-surface:var(--color-well)]">
              <svg
                aria-hidden="true"
                viewBox={`0 0 ${W} ${H}`}
                preserveAspectRatio="none"
                className="absolute inset-0 block size-full overflow-visible"
              >
                {bands.map((run) => (
                  <path
                    key={`b${run[0]?.week}`}
                    data-part="band"
                    d={bandPath(run, x, y)}
                    className="fill-pencil/25"
                  />
                ))}
                {events.map((ev) => (
                  <line
                    key={`e${ev.date}`}
                    data-part="event"
                    x1={x(ev.date)}
                    x2={x(ev.date)}
                    y1={0}
                    y2={H}
                    strokeWidth={1}
                    strokeDasharray="3 3"
                    vectorEffect="non-scaling-stroke"
                    className="stroke-pencil"
                  />
                ))}
                {lines.map((run) => (
                  <path
                    key={`l${run[0]?.week}`}
                    data-part="segment"
                    d={
                      run.length === 1
                        ? dot(x(run[0]?.week ?? range.from), y(run[0]?.value ?? 0))
                        : run
                            .map(
                              (p, i) =>
                                `${i === 0 ? "M" : "L"}${n2(x(p.week))},${n2(y(p.value ?? 0))}`,
                            )
                            .join("")
                    }
                    fill="none"
                    strokeWidth={run.length === 1 ? 4 : 2}
                    strokeLinejoin="round"
                    strokeLinecap="round"
                    vectorEffect="non-scaling-stroke"
                    className="stroke-ink"
                  />
                ))}
                {hollow.map((p) => (
                  <g key={`h${p.week}`} data-part="hollow">
                    <path
                      d={dot(x(p.week), y(p.value ?? 0))}
                      strokeWidth={8}
                      strokeLinecap="round"
                      vectorEffect="non-scaling-stroke"
                      className="stroke-ink-soft"
                    />
                    <path
                      d={dot(x(p.week), y(p.value ?? 0))}
                      strokeWidth={4.5}
                      strokeLinecap="round"
                      vectorEffect="non-scaling-stroke"
                      className="stroke-well"
                    />
                  </g>
                ))}
                {last ? (
                  <g data-part="last">
                    <path
                      d={dot(x(last.week), y(last.value ?? 0))}
                      strokeWidth={10}
                      strokeLinecap="round"
                      vectorEffect="non-scaling-stroke"
                      className="stroke-well"
                    />
                    <path
                      d={dot(x(last.week), y(last.value ?? 0))}
                      strokeWidth={6}
                      strokeLinecap="round"
                      vectorEffect="non-scaling-stroke"
                      className="stroke-ink"
                    />
                  </g>
                ) : null}
              </svg>
              {events.map((ev, i) => (
                <span
                  key={`n${ev.date}`}
                  className="absolute top-0.5 -translate-x-1/2 rounded-pill bg-well px-1 text-xs leading-none text-ink-soft"
                  style={{ left: pc(x(ev.date)), ...LINING }}
                >
                  {i + 1}
                </span>
              ))}
            </div>
            <div className="flex justify-between text-xs text-ink-faint" style={LINING}>
              <span>{shortDayText(range.from)}</span>
              <span>{dayText(range.to)}</span>
            </div>
          </div>
        </div>
      ) : (
        <p className="m-0 rounded-field bg-well px-3 py-4 text-sm text-ink-soft">
          No {unit.many} in this range yet
        </p>
      )}
    </figure>
  );
}

/** The key, drawn with the marks' own shapes, so colour is never the only cue. */
function Key({ minN, events }: { minN: number; events: boolean }) {
  return (
    <ul
      aria-label="Key"
      className="m-0 flex list-none flex-wrap gap-x-5 gap-y-1 p-0 text-sm text-ink-soft"
    >
      <li className="flex items-center gap-2">
        <svg aria-hidden="true" width={18} height={10}>
          <path d="M1,7L9,3L17,5" fill="none" strokeWidth={2} className="stroke-ink" />
        </svg>
        Weekly value, {minN} or more
      </li>
      <li className="flex items-center gap-2">
        <svg aria-hidden="true" width={10} height={10}>
          <circle cx={5} cy={5} r={3.2} strokeWidth={1.75} className="fill-none stroke-ink-soft" />
        </svg>
        Fewer than {minN}, off the line
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="h-2.5 w-4 rounded-(--radius-board) bg-pencil/25" />
        Stored 80% range
      </li>
      {events ? (
        <li className="flex items-center gap-2">
          <svg aria-hidden="true" width={6} height={14}>
            <line
              x1={3}
              x2={3}
              y1={0}
              y2={14}
              strokeWidth={1}
              strokeDasharray="3 3"
              className="stroke-pencil"
            />
          </svg>
          Report
        </li>
      ) : null}
    </ul>
  );
}

/**
 * KPI trends as small multiples on one shared time axis (ADR 0072). Each multiple draws its stored
 * band where `low` and `high` exist, exactly as given; a line through the weeks with at least
 * `minN` behind them; hollow dots for thinner weeks, off the line; and gaps, never zeros, for weeks
 * with no value. Report dates are dashed `pencil` rules, numbered and listed under the grid. Each
 * multiple carries an accessible summary that names every week, gaps included. Nothing animates.
 */
export function KpiTrendGrid({ items, range, events = [], minN, className }: KpiTrendGridProps) {
  const sorted = [...events].sort((a, b) => isoDay(a.date) - isoDay(b.date));
  return (
    <div className={cn("@container flex flex-col gap-5", className)}>
      <Key minN={minN} events={sorted.length > 0} />
      <div className="grid grid-cols-1 gap-x-8 gap-y-7 @min-[640px]:grid-cols-2 @min-[1000px]:grid-cols-3">
        {items.map((item) => (
          <Multiple key={item.metric} item={item} range={range} events={sorted} minN={minN} />
        ))}
      </div>
      {sorted.length > 0 ? (
        <ol
          aria-label="Reports"
          className="m-0 flex list-none flex-col gap-1 p-0 text-sm text-ink-soft"
        >
          {sorted.map((ev, i) => (
            <li key={ev.date} className="flex items-center gap-2" style={LINING}>
              <span className="w-4 text-right font-medium text-ink">{i + 1}</span>
              <span>
                <time dateTime={ev.date.slice(0, 10)}>{dayText(ev.date)}</time>: {ev.label}
              </span>
            </li>
          ))}
        </ol>
      ) : null}
    </div>
  );
}
