"use client";

import { cn } from "../lib/index.js";
import { dur, ease, motion, useReducedMotion } from "../motion/index.js";

export interface ProgressPlaceholderProps {
  /** The shape of the section this stands in for. */
  shape: "graph" | "stat" | "list" | "board";
  verb: "Analysing" | "Importing";
  /** Omit for "no count yet". Without a `total` the count reads "so far" and the stroke idles. */
  progress?: { done: number; total?: number; unit: string };
  /** A quiet line under the status, for example what the numbers will be based on. */
  basisNote?: string;
}

/** The status text: the one line a screen reader announces and a sighted reader reads. */
export function progressText(
  verb: ProgressPlaceholderProps["verb"],
  progress: ProgressPlaceholderProps["progress"],
): string {
  if (!progress) return verb;
  if (progress.total === undefined) return `${verb}: ${progress.done} ${progress.unit} so far`;
  return `${verb} ${progress.done} / ${progress.total} ${progress.unit}`;
}

/** done / total clamped to 0..1; a zero total is no progress, not NaN. */
export function fraction(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(1, Math.max(0, done / total));
}

/** Faint writing lines: the ruled sheet every shape sits on. */
const RULED =
  "bg-[repeating-linear-gradient(to_bottom,transparent_0,transparent_25px,var(--color-rule)_25px,var(--color-rule)_26px)]";

/** A static skeleton in the shape of what will be there, drawn in pencil, never a spinner (B18). */
function Skeleton({ shape }: { shape: ProgressPlaceholderProps["shape"] }) {
  const line = "stroke-pencil/50 fill-none [stroke-linecap:round] [stroke-linejoin:round]";
  switch (shape) {
    case "graph":
      return (
        <svg viewBox="0 0 240 80" className="block h-28 w-full" aria-hidden="true">
          <line
            x1="0"
            y1="40"
            x2="240"
            y2="40"
            className="stroke-rule-strong/60"
            strokeDasharray="2 4"
          />
          <polyline
            points="4,44 28,38 52,50 76,30 100,36 124,24 148,46 172,34 196,40 220,28"
            className={line}
            vectorEffect="non-scaling-stroke"
            strokeWidth="1.5"
            strokeDasharray="3 5"
          />
        </svg>
      );
    case "stat":
      return (
        <div className="flex flex-col gap-3 py-2" aria-hidden="true">
          <span className="h-9 w-24 rounded-field bg-well" />
          <span className="h-1.5 w-full rounded-pill bg-well" />
          <span className="h-3 w-40 rounded-pill bg-well" />
        </div>
      );
    case "list":
      return (
        <ul className="m-0 flex list-none flex-col p-0" aria-hidden="true">
          {[72, 56, 64, 48].map((w) => (
            <li key={w} className="flex h-[34px] items-center gap-3 border-b border-rule/60">
              <span className="h-2.5 w-6 rounded-pill bg-well" />
              <span className="h-2.5 rounded-pill bg-well" style={{ width: `${w}%` }} />
            </li>
          ))}
        </ul>
      );
    case "board":
      return (
        <svg
          viewBox="0 0 80 80"
          className="mx-auto block aspect-square w-full max-w-60"
          aria-hidden="true"
        >
          <rect
            x="0.5"
            y="0.5"
            width="79"
            height="79"
            rx="1.5"
            className="stroke-rule-strong/60 fill-none"
          />
          {[1, 2, 3, 4, 5, 6, 7].map((i) => (
            <g key={i} className="stroke-rule">
              <line x1={i * 10} y1="0" x2={i * 10} y2="80" vectorEffect="non-scaling-stroke" />
              <line x1="0" y1={i * 10} x2="80" y2={i * 10} vectorEffect="non-scaling-stroke" />
            </g>
          ))}
        </svg>
      );
  }
}

export function ProgressPlaceholder({
  shape,
  verb,
  progress,
  basisNote,
}: ProgressPlaceholderProps) {
  const reduced = useReducedMotion();
  const known = progress?.total !== undefined;
  const f = known && progress ? fraction(progress.done, progress.total ?? 0) : 0;

  return (
    <div
      role="status"
      aria-live="polite"
      data-shape={shape}
      className="flex flex-col gap-4 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className={cn("rounded-field px-2", RULED)}>
        <Skeleton shape={shape} />
      </div>
      <div className="flex flex-col gap-1.5">
        <p className="m-0 font-serif text-base text-ink">{progressText(verb, progress)}</p>
        {/* The pencil stroke: it grows with the count and soaks forward like ink (DESIGN, Progress). */}
        <div aria-hidden="true" className="relative h-[3px] overflow-hidden rounded-pill bg-well">
          {known ? (
            <motion.span
              data-part="stroke"
              data-fraction={f}
              className="absolute inset-0 origin-left rounded-pill bg-pencil"
              initial={false}
              animate={{ scaleX: f }}
              transition={reduced ? { duration: 0 } : { duration: dur.soak, ease: ease.soak }}
            />
          ) : (
            !reduced && (
              <motion.span
                className="absolute inset-y-0 left-0 w-1/4 rounded-pill bg-pencil/60"
                animate={{ x: ["-100%", "400%"] }}
                transition={{
                  duration: dur.soak * 2.5,
                  ease: ease.soak,
                  repeat: Number.POSITIVE_INFINITY,
                }}
              />
            )
          )}
        </div>
        {basisNote ? <p className="m-0 text-xs text-ink-faint">{basisNote}</p> : null}
      </div>
    </div>
  );
}
