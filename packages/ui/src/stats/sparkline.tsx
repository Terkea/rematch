"use client";

import { Shape } from "@visx/visx";
import { type CSSProperties, useEffect, useId } from "react";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";

export interface SparklineProps {
  /** One value per snapshot, oldest first. `null` (no games in that window) breaks the line. */
  values: (number | null)[];
  /** Accessible name: what the line is and where it goes, for example "Accuracy, 69.0% to 74.2%". */
  label: string;
  /** Mark the latest value with an `ink` dot (default true). */
  highlightLast?: boolean;
  /**
   * The least vertical span, in the values' units, centred on the series range. KpiStat passes
   * twice the current 80% interval width, so a move smaller than the uncertainty looks small.
   */
  minSpan?: number;
  /** Draw the line in from the left on mount over `dur.reveal` (skipped under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

/** The drawing box in px. The SVG stretches to its column; strokes and dots never scale. */
const W = 120;
const H = 32;
/** Room for the last dot and its halo, so nothing clips at the box edge. */
const INSET = 5;
/** Dots fade in over this many px of the reveal as the clip edge passes them. */
const FADE = 8;
/** The halo reads the host's surface, as `--eval-graph-surface` does. */
const HALO = "stroke-[var(--stat-surface,var(--color-sheet))]";

type Pt = { v: number | null; i: number };

/**
 * A trend line with no axes: the shape of a KPI over time, not its values (KpiStat states those).
 * Nulls break the line rather than bridging a gap with invented data; a lone value between gaps is
 * a small pencil dot.
 */
export function Sparkline({
  values,
  label,
  highlightLast = true,
  minSpan = 0,
  drawIn = false,
  className,
}: SparklineProps) {
  const reduced = useReducedMotion();
  const clipId = `sp${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const numbers = values.filter((v): v is number => v !== null);
  const enough = numbers.length >= 2;

  // One clock: a single clip sweeps left to right, so the subpaths after a gap reveal in order, and
  // each dot fades in as the sweep reaches it.
  const revealing = drawIn && !reduced && enough;
  const progress = useMotionValue(revealing ? 0 : 1);
  useEffect(() => {
    if (!revealing) {
      progress.jump(1);
      return;
    }
    progress.jump(0);
    const controls = animate(progress, 1, { duration: dur.reveal, ease: ease.pen });
    return () => controls.stop();
  }, [revealing, progress]);

  if (!enough) {
    return (
      <span
        role="img"
        aria-label={`${label}: not enough games yet`}
        className={cn("block h-8 text-xs text-ink-faint", className)}
      >
        <span aria-hidden="true">Not enough games yet</span>
      </span>
    );
  }

  const seriesMin = Math.min(...numbers);
  const seriesMax = Math.max(...numbers);
  const mid = (seriesMin + seriesMax) / 2;
  const span = Math.max(seriesMax - seriesMin, minSpan);
  const min = span > seriesMax - seriesMin ? mid - span / 2 : seriesMin;
  const max = span > seriesMax - seriesMin ? mid + span / 2 : seriesMax;
  const step = values.length > 1 ? (W - 2 * INSET) / (values.length - 1) : 0;
  const x = (i: number) => INSET + i * step;
  // A flat series with no minSpan sits mid-height rather than on an edge.
  const y = (v: number) => (span === 0 ? H / 2 : INSET + (1 - (v - min) / span) * (H - 2 * INSET));

  const pts: Pt[] = values.map((v, i) => ({ v, i }));
  const d =
    Shape.line<Pt>({
      x: (p) => x(p.i),
      y: (p) => y(p.v ?? 0),
      defined: (p) => p.v !== null,
    })(pts) ?? "";
  const lone = pts.filter(
    (p) => p.v !== null && values[p.i - 1] == null && values[p.i + 1] == null,
  );
  let lastIndex = values.length - 1;
  while (lastIndex >= 0 && values[lastIndex] === null) lastIndex--;
  const lastValue = values[lastIndex] ?? null;

  const dot = (p: { i: number; v: number }) => `M${x(p.i)},${y(p.v)}h0`;
  const dotStyle = (i: number): CSSProperties => ({
    opacity:
      `clamp(0, calc((var(--draw, 1) * ${W + FADE} - ${x(i).toFixed(1)}) / ${FADE}), 1)` as unknown as number,
  });

  return (
    <motion.svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      data-min={min}
      data-max={max}
      className={cn("block h-8 w-full overflow-visible", className)}
      style={{ "--draw": progress } as never}
    >
      <defs>
        <clipPath id={clipId}>
          <rect
            x={-INSET}
            y={-INSET}
            width={W + 2 * INSET}
            height={H + 2 * INSET}
            style={{ transform: "scaleX(var(--draw, 1))", transformOrigin: `${-INSET}px 0` }}
          />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clipId})`}>
        <path
          data-part="line"
          d={d}
          fill="none"
          strokeWidth={1.5}
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
          className="stroke-pencil"
        />
      </g>
      {lone.map((p) => (
        <path
          key={p.i}
          d={dot({ i: p.i, v: p.v ?? 0 })}
          strokeWidth={3}
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
          className="stroke-pencil"
          style={dotStyle(p.i)}
        />
      ))}
      {highlightLast && lastValue !== null && (
        <g style={dotStyle(lastIndex)}>
          {/* The halo rule: the dot's neighbour is always the surface, never the line. */}
          <path
            d={dot({ i: lastIndex, v: lastValue })}
            strokeWidth={10}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            className={HALO}
          />
          <path
            data-part="last"
            d={dot({ i: lastIndex, v: lastValue })}
            strokeWidth={6}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            className="stroke-ink"
          />
        </g>
      )}
    </motion.svg>
  );
}
