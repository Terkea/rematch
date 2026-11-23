"use client";

import { Responsive, Scale, Shape } from "@visx/visx";
import { type KeyboardEvent, type PointerEvent, useId, useMemo, useState } from "react";
import { cn } from "../lib/index.js";
import { dateTicks, TREND_AXIS, TREND_HEIGHT, TREND_RIGHT } from "./scales.js";

// The rating trajectory (F6, ADR 0088): the player's pre-game rating per rated game as one ink
// line, the peak marked and labelled, the current rating at the end, and the two windows the page
// compares shaded and named on the plot. One series, so no legend: the title names it. A table of
// the months is the chart's text view; a crosshair readout follows the pointer and the arrow keys.

export interface RatingTrajectoryPoint {
  id: string;
  /** ISO date-time of the game. */
  date: string;
  /** The player's rating before the game. */
  rating: number;
}

export interface RatingTrajectoryMark {
  date: string;
  rating: number;
  /** Drawn beside the mark: "Peak 1,097, April 2026", "Now 816". */
  label: string;
}

export interface RatingTrajectoryWindow {
  from: string;
  to: string;
  label: string;
}

export interface RatingTrajectoryProps {
  points: RatingTrajectoryPoint[];
  peak: RatingTrajectoryMark;
  current: RatingTrajectoryMark;
  /** The compared windows ("then" and "now"), shaded on the plot. */
  windows: RatingTrajectoryWindow[];
  /** The chart's accessible name; the table is "<label>, by month". */
  label: string;
  className?: string;
}

/** Room above the plot for the window labels. */
const TOP = 24;
/** The rating labels' column ("1,100" at text-xs). */
const LEFT = 40;
const PLOT_H = TREND_HEIGHT - TOP - TREND_AXIS;
const STEPS = [25, 50, 100, 200, 500, 1000];
const MAX_TICKS = 5;
const SURFACE = "var(--chart-surface, var(--color-sheet))";
const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const count = new Intl.NumberFormat("en-GB");

/** The y domain rounded out to the smallest step that gives at most five gridlines. */
export function ratingDomain(ratings: number[]): { lo: number; hi: number; ticks: number[] } {
  const clean = ratings.filter((r) => Number.isFinite(r));
  const min = clean.length ? Math.min(...clean) : 0;
  const max = clean.length ? Math.max(...clean) : 0;
  for (const step of STEPS) {
    let lo = Math.floor(min / step) * step;
    let hi = Math.ceil(max / step) * step;
    if (lo === hi) {
      lo -= step;
      hi += step;
    }
    if ((hi - lo) / step + 1 <= MAX_TICKS || step === STEPS.at(-1)) {
      const ticks: number[] = [];
      for (let t = lo; t <= hi; t += step) ticks.push(t);
      return { lo, hi, ticks };
    }
  }
  return { lo: 0, hi: 1, ticks: [] };
}

export type MonthRow = { month: string; label: string; games: number; last: number; high: number };

/** Each UTC month: its games, its last rating and its highest, oldest first. */
export function monthlyRatings(points: RatingTrajectoryPoint[]): MonthRow[] {
  const sorted = points
    .filter((p) => Number.isFinite(p.rating) && Number.isFinite(new Date(p.date).getTime()))
    .sort((a, b) => new Date(a.date).getTime() - new Date(b.date).getTime());
  const rows = new Map<string, MonthRow>();
  for (const p of sorted) {
    const d = new Date(p.date);
    const month = d.toISOString().slice(0, 7);
    const row = rows.get(month);
    if (row) {
      row.games += 1;
      row.last = p.rating;
      row.high = Math.max(row.high, p.rating);
    } else {
      rows.set(month, {
        month,
        label: `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`,
        games: 1,
        last: p.rating,
        high: p.rating,
      });
    }
  }
  return [...rows.values()];
}

const dayText = (d: Date) => `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;

function Placeholder({ ticks, y }: { ticks: number[]; y: (r: number) => number }) {
  return (
    <div aria-hidden="true" data-part="trajectory-placeholder" className="absolute inset-0">
      {ticks.map((t) => (
        <div
          key={t}
          className="absolute right-0 left-0 flex -translate-y-1/2 items-center"
          style={{ top: y(t) }}
        >
          <span
            className="shrink-0 pr-1.5 text-right font-sans text-xs text-ink-faint tabular-nums"
            style={{ width: LEFT }}
          >
            {count.format(t)}
          </span>
          <span className="h-px flex-1 bg-rule" />
        </div>
      ))}
    </div>
  );
}

export function RatingTrajectory({
  points,
  peak,
  current,
  windows,
  label,
  className,
}: RatingTrajectoryProps) {
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const { parentRef, width } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });
  const sorted = useMemo(
    () =>
      points
        .map((p) => ({ ...p, time: new Date(p.date) }))
        .filter((p) => Number.isFinite(p.rating) && Number.isFinite(p.time.getTime()))
        .sort((a, b) => a.time.getTime() - b.time.getTime()),
    [points],
  );
  const domain = useMemo(
    () => ratingDomain([...sorted.map((p) => p.rating), peak.rating, current.rating]),
    [sorted, peak.rating, current.rating],
  );
  const y = useMemo(
    () =>
      Scale.scaleLinear<number>({
        domain: [domain.lo, domain.hi],
        range: [TOP + PLOT_H, TOP],
        clamp: true,
      }),
    [domain],
  );
  const months = useMemo(() => monthlyRatings(points), [points]);

  const geo = useMemo(() => {
    if (width <= 0 || sorted.length === 0) return null;
    const times = [...sorted.map((p) => p.time.getTime()), new Date(current.date).getTime()].filter(
      Number.isFinite,
    );
    const x = Scale.scaleUtc<number>({
      domain: [new Date(Math.min(...times)), new Date(Math.max(...times))],
      range: [LEFT, Math.max(LEFT + 1, width - TREND_RIGHT)],
      clamp: true,
    });
    const placed = sorted.map((p) => ({ ...p, x: x(p.time), y: y(p.rating) }));
    const end = { x: x(new Date(current.date)), y: y(current.rating) };
    const line =
      Shape.line<{ x: number; y: number }>({ x: (p) => p.x, y: (p) => p.y })([...placed, end]) ??
      "";
    const peakAt = { x: x(new Date(peak.date)), y: y(peak.rating) };
    const spans = windows.map((w) => {
      const x0 = x(new Date(w.from));
      const x1 = x(new Date(w.to));
      return { ...w, x0, x1: Math.max(x1, x0 + 2) };
    });
    // Labels sit inside the plot: anchored inward near either edge.
    const anchorFor = (px: number) =>
      px > width - 120
        ? ("end" as const)
        : px < LEFT + 60
          ? ("start" as const)
          : ("middle" as const);
    return { x, placed, end, line, peakAt, spans, ticks: dateTicks(x, width), anchorFor };
  }, [width, sorted, current.date, current.rating, peak.date, peak.rating, windows, y]);

  const [active, setActive] = useState<number | null>(null);
  const shown = active !== null ? geo?.placed[active] : undefined;

  const nearest = (px: number) => {
    if (!geo || geo.placed.length === 0) return null;
    let best = 0;
    for (let i = 1; i < geo.placed.length; i++) {
      if (Math.abs((geo.placed[i]?.x ?? 0) - px) < Math.abs((geo.placed[best]?.x ?? 0) - px)) {
        best = i;
      }
    }
    return best;
  };
  const onMove = (event: PointerEvent<HTMLElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    setActive(nearest(event.clientX - box.left));
  };
  const onKey = (event: KeyboardEvent<HTMLElement>) => {
    if (!geo) return;
    const last = geo.placed.length - 1;
    const from = active ?? last;
    const next =
      event.key === "ArrowLeft"
        ? Math.max(0, from - 1)
        : event.key === "ArrowRight"
          ? Math.min(last, from + 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (next === null) return;
    event.preventDefault();
    setActive(next);
  };

  const plotBottom = TOP + PLOT_H;
  const peakAnchor = geo ? geo.anchorFor(geo.peakAt.x) : "middle";
  const summary = `${label}: ${count.format(sorted.length)} rated games. ${peak.label}. ${current.label}.`;

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <p className="sr-only">{summary}</p>
      <div ref={parentRef} className="relative h-52 w-full">
        {!geo && <Placeholder ticks={domain.ticks} y={y} />}
        {geo && (
          <>
            <svg
              role="img"
              aria-label={label}
              width={width}
              height={TREND_HEIGHT}
              className="absolute inset-0 block overflow-visible"
            >
              {geo.spans.map((s) => (
                <g key={`${s.label}-${s.from}`} data-part="window">
                  <rect
                    x={s.x0}
                    y={TOP}
                    width={s.x1 - s.x0}
                    height={PLOT_H}
                    className="fill-pencil/25 dark:fill-pencil/40"
                  />
                  <text
                    x={
                      geo.anchorFor((s.x0 + s.x1) / 2) === "end"
                        ? s.x1
                        : geo.anchorFor((s.x0 + s.x1) / 2) === "start"
                          ? s.x0
                          : (s.x0 + s.x1) / 2
                    }
                    y={TOP - 8}
                    textAnchor={geo.anchorFor((s.x0 + s.x1) / 2)}
                    className="fill-ink-soft font-sans text-xs"
                  >
                    {s.label}
                  </text>
                </g>
              ))}
              {domain.ticks.map((t) => (
                <g key={t} data-tick-y={t}>
                  <line
                    x1={LEFT}
                    x2={width}
                    y1={y(t)}
                    y2={y(t)}
                    strokeWidth={1}
                    className="stroke-rule"
                  />
                  <text
                    x={LEFT - 6}
                    y={y(t) + 4}
                    textAnchor="end"
                    className="fill-ink-faint font-sans text-xs tabular-nums"
                  >
                    {count.format(t)}
                  </text>
                </g>
              ))}
              <line
                x1={LEFT}
                x2={width}
                y1={plotBottom}
                y2={plotBottom}
                strokeWidth={1}
                className="stroke-rule-strong"
              />
              {geo.ticks.map((t) => (
                <text
                  key={t.iso}
                  x={t.anchor === "middle" ? Math.min(Math.max(t.x, LEFT + 18), width - 24) : t.x}
                  y={plotBottom + 18}
                  textAnchor={t.anchor}
                  className="fill-ink-faint font-sans text-xs tabular-nums"
                >
                  {t.text}
                </text>
              ))}
              <path
                d={geo.line}
                fill="none"
                strokeWidth={5}
                strokeLinejoin="round"
                strokeLinecap="round"
                style={{ stroke: SURFACE }}
              />
              <path
                data-part="line"
                d={geo.line}
                fill="none"
                strokeWidth={2}
                strokeLinejoin="round"
                strokeLinecap="round"
                className="stroke-ink"
              />
              {shown && (
                <line
                  data-part="crosshair"
                  x1={shown.x}
                  x2={shown.x}
                  y1={TOP}
                  y2={plotBottom}
                  strokeWidth={1}
                  className="stroke-ink-soft"
                />
              )}
              <g data-part="peak" transform={`translate(${geo.peakAt.x} ${geo.peakAt.y})`}>
                <circle r={6.5} style={{ fill: SURFACE }} />
                <circle r={4.5} className="fill-ink" />
              </g>
              <text
                x={geo.peakAt.x + (peakAnchor === "start" ? 8 : peakAnchor === "end" ? -8 : 0)}
                y={Math.max(TOP + 12, geo.peakAt.y - 12)}
                textAnchor={peakAnchor}
                className="fill-ink font-sans text-xs font-medium tabular-nums"
                style={{ paintOrder: "stroke", stroke: SURFACE, strokeWidth: 4 }}
              >
                {peak.label}
              </text>
              <g data-part="current" transform={`translate(${geo.end.x} ${geo.end.y})`}>
                <circle r={5.5} style={{ fill: SURFACE }} />
                <circle r={3.5} className="fill-ink" />
              </g>
              <text
                x={geo.end.x - 8}
                y={geo.end.y > plotBottom - 20 ? geo.end.y - 10 : geo.end.y + 18}
                textAnchor="end"
                className="fill-ink font-sans text-xs font-medium tabular-nums"
                style={{ paintOrder: "stroke", stroke: SURFACE, strokeWidth: 4 }}
              >
                {current.label}
              </text>
              {shown && (
                <circle
                  cx={shown.x}
                  cy={shown.y}
                  r={4}
                  strokeWidth={2}
                  className="fill-sheet stroke-ink"
                />
              )}
            </svg>
            {/* The one pointer and keyboard target: the nearest game to the pointer is read out. */}
            <div
              data-part="hit"
              role="slider"
              tabIndex={0}
              aria-label={`${label}: step through the games`}
              aria-valuemin={domain.lo}
              aria-valuemax={domain.hi}
              aria-valuenow={shown ? shown.rating : current.rating}
              aria-valuetext={
                shown ? `${dayText(shown.time)}: ${count.format(shown.rating)}` : current.label
              }
              aria-describedby={`${uid}-readout`}
              className="absolute inset-0 rounded-field outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink"
              onPointerMove={onMove}
              onPointerLeave={() => setActive(null)}
              onKeyDown={onKey}
              onBlur={() => setActive(null)}
            />
            <span id={`${uid}-readout`} className="sr-only" aria-live="polite">
              {shown ? `${dayText(shown.time)}: ${count.format(shown.rating)}` : ""}
            </span>
            {shown && (
              <div
                data-part="readout"
                aria-hidden="true"
                className="pointer-events-none absolute top-0 rounded-field border border-rule-strong bg-sheet px-2 py-1 font-sans text-xs text-ink tabular-nums shadow-lift"
                style={{
                  left: Math.min(Math.max(shown.x - 60, 0), Math.max(0, width - 120)),
                }}
              >
                {`${dayText(shown.time)}: ${count.format(shown.rating)}`}
              </div>
            )}
          </>
        )}
      </div>
      <details className="text-sm text-ink-soft">
        <summary className="cursor-pointer rounded-field font-medium text-ink outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink">
          Show as a table
        </summary>
        <table className="mt-2 w-full border-collapse text-left">
          <caption className="sr-only">{`${label}, by month`}</caption>
          <thead>
            <tr>
              <th scope="col" className="py-1 pr-4 font-medium text-ink-soft">
                Month
              </th>
              <th scope="col" className="py-1 pr-4 font-medium text-ink-soft">
                Rated games
              </th>
              <th scope="col" className="py-1 pr-4 font-medium text-ink-soft">
                Last rating
              </th>
              <th scope="col" className="py-1 font-medium text-ink-soft">
                Highest
              </th>
            </tr>
          </thead>
          <tbody className="tabular-nums">
            {months.map((m) => (
              <tr key={m.month} className="border-t border-rule">
                <th scope="row" className="py-1 pr-4 font-normal text-ink">
                  {m.label}
                </th>
                <td className="py-1 pr-4">{count.format(m.games)}</td>
                <td className="py-1 pr-4">{count.format(m.last)}</td>
                <td className="py-1">{count.format(m.high)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}
