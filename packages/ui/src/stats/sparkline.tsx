"use client";

import { Shape } from "@visx/visx";
import { type CSSProperties, useEffect } from "react";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";

export interface SparklineProps {
  /** One value per snapshot, oldest first. `null` (no games in that window) breaks the line. */
  values: (number | null)[];
  /** Accessible name: what the line is and where it goes, for example "Accuracy, 69.0% to 74.2%". */
  label: string;
  /** Mark the latest value with an `ink` dot (default true). */
  highlightLast?: boolean;
  /** Draw the line in from the left on mount over `dur.reveal` (skipped under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

/** The drawing box in px. The SVG stretches to its column; strokes and dots never scale. */
const W = 120;
const H = 32;
/** Room for the last dot and its halo, so nothing clips at the box edge. */
const INSET = 5;

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
  drawIn = false,
  className,
}: SparklineProps) {
  const reduced = useReducedMotion();
  const numbers = values.filter((v): v is number => v !== null);
  const enough = numbers.length >= 2;

  // One clock: the line's dash offset and the last dot's opacity both follow `--draw`.
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
        aria-label="Not enough games yet"
        className={cn("block h-8 text-xs text-ink-faint", className)}
      >
        <span aria-hidden="true">Not enough games yet</span>
      </span>
    );
  }

  const min = Math.min(...numbers);
  const max = Math.max(...numbers);
  const span = max - min;
  const step = values.length > 1 ? (W - 2 * INSET) / (values.length - 1) : 0;
  const x = (i: number) => INSET + i * step;
  // A flat series sits mid-height rather than on an edge.
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
  const lineStyle: CSSProperties = revealing
    ? { strokeDasharray: 1, strokeDashoffset: "calc(1 - var(--draw, 1))" }
    : {};
  const dotStyle: CSSProperties = {
    opacity: "clamp(0, calc((var(--draw, 1) - 0.85) * 8), 1)" as unknown as number,
  };

  return (
    <motion.svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      className={cn("block h-8 w-full overflow-visible", className)}
      style={{ "--draw": progress } as never}
    >
      <path
        data-part="line"
        d={d}
        pathLength={1}
        fill="none"
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
        className="stroke-pencil"
        style={lineStyle}
      />
      {lone.map((p) => (
        <path
          key={p.i}
          d={dot({ i: p.i, v: p.v ?? 0 })}
          strokeWidth={3}
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
          className="stroke-pencil"
          style={dotStyle}
        />
      ))}
      {highlightLast && lastValue !== null && (
        <g style={dotStyle}>
          {/* The halo rule: the dot's neighbour is always the surface, never the line. */}
          <path
            d={dot({ i: lastIndex, v: lastValue })}
            strokeWidth={10}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            className="stroke-[var(--stat-surface,var(--color-sheet))]"
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
