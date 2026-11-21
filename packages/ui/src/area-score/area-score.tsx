"use client";

import { type CSSProperties, useEffect } from "react";
import type { Area, AreaScoreProps } from "../fixtures/types.js";
import { cn } from "../lib/index.js";
import {
  animate,
  dur,
  ease,
  type MotionValue,
  motion,
  useMotionValue,
  useReducedMotion,
} from "../motion/index.js";

export type { Area, AreaScoreProps } from "../fixtures/types.js";

export const areaLabels: Record<Area, string> = {
  openings: "Openings",
  middlegame: "Middlegame",
  endgame: "Endgame",
  tactics: "Tactics",
  calculation: "Calculation",
  time: "Time",
  mindset: "Mindset",
};

const NUMERALS = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
const clamp = (n: number) => Math.min(100, Math.max(0, n));
/** Minor ruling every 10 points, a longer tick at 0, 50 and 100. */
const RULING = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

/** "+4 since last report", "−5 since last report" (U+2212) or "No change since last report". */
export function deltaText(delta: number): string {
  if (delta === 0) return "No change since last report";
  return `${delta > 0 ? "+" : "−"}${Math.abs(delta)} since last report`;
}

/** "Endgame 62, band median 55, next band 68". */
export function areaScoreText({ area, score, band }: AreaScoreProps): string {
  return `${areaLabels[area]} ${score}, band median ${band.median}, next band ${band.next}`;
}

const pos = (n: number): CSSProperties => ({ left: `${clamp(n)}%` });

/**
 * The ruler: a 0 to 100 scale in `rule-strong`, ruled every 10 in `rule`. Three marks, each with
 * its own shape so none relies on colour: the band median is a short `pencil` tick hanging below
 * the scale, the next band a `good` tick standing above it with a flag, and the player a tall
 * `ink` needle through both, on a `sheet` halo.
 */
function Ruler({ score, band }: Pick<AreaScoreProps, "score" | "band">) {
  return (
    <div aria-hidden="true" className="relative h-9 w-full">
      <div className="absolute inset-x-0 top-1/2 h-px bg-rule-strong" />
      {RULING.map((n) => (
        <div
          key={n}
          className={cn(
            "absolute top-1/2 w-px -translate-x-1/2",
            n % 50 === 0 ? "h-2.5 -mt-1.25 bg-rule-strong" : "h-1.5 -mt-0.75 bg-rule",
          )}
          style={pos(n)}
        />
      ))}
      {/* The gap between the median and the player: an ink tint that fades in with the sweep. */}
      <div
        data-gap
        className="absolute top-1/2 -mt-1 h-2 bg-ink/15"
        style={{
          left: `${clamp(Math.min(score, band.median))}%`,
          width: `${Math.abs(clamp(score) - clamp(band.median))}%`,
          opacity: "var(--sweep, 1)" as unknown as number,
        }}
      />
      <div
        data-tick="median"
        className="absolute top-1/2 h-3 w-0.5 -translate-x-1/2 rounded-full bg-pencil"
        style={pos(band.median)}
      />
      <div
        data-tick="next"
        className="absolute top-1/2 -mt-3.5 h-3.5 w-0.5 -translate-x-1/2 bg-good"
        style={pos(band.next)}
      >
        <span className="absolute top-0 left-0.5 h-1.5 w-1.5 bg-good [clip-path:polygon(0_0,100%_50%,0_100%)]" />
      </div>
      {/*
        The needle rides a full-width carrier that starts one ruler-width to the left, so the sweep
        is a transform of `--sweep × score%` of the ruler (M9), never an animated `left`. `--sweep`
        is the list's one clock: 1 at rest, 0 to 1 during a draw-in.
      */}
      <div
        className="absolute inset-y-0 -left-full w-full"
        style={{ transform: `translateX(calc(var(--sweep, 1) * ${clamp(score)}%))` }}
      >
        <div
          data-tick="score"
          className="absolute inset-y-0.5 right-0 w-[3px] translate-x-1/2 rounded-full bg-ink outline-2 outline-sheet"
        />
      </div>
    </div>
  );
}

function ScoreRow(props: AreaScoreProps) {
  const { area, score, band, delta, basis } = props;
  return (
    <>
      <span className="sr-only">{areaScoreText(props)}</span>
      <div className="grid grid-cols-[1fr_auto] items-baseline gap-x-4 @min-[560px]:contents">
        <div className="flex flex-col @min-[560px]:col-start-1 @min-[560px]:row-start-1">
          <span data-area-label aria-hidden="true" className="font-medium text-ink">
            {areaLabels[area]}
          </span>
          <span className="text-sm text-ink-faint">
            {basis.games} {basis.timeClass} games
          </span>
        </div>
        <span
          aria-hidden="true"
          className="text-right font-serif text-3xl font-semibold text-ink @min-[560px]:col-start-2 @min-[560px]:row-start-1"
          style={NUMERALS}
        >
          {score}
        </span>
      </div>
      <div className="flex flex-col gap-0.5 @min-[560px]:col-start-3 @min-[560px]:row-start-1">
        <Ruler score={score} band={band} />
        {delta !== undefined ? (
          <span
            className={cn("text-sm", delta > 0 ? "text-good-text" : "text-ink-soft")}
            style={NUMERALS}
          >
            {deltaText(delta)}
          </span>
        ) : (
          // No earlier report: keep the row's height so the rulers stay evenly spaced.
          <span aria-hidden="true" className="h-5" />
        )}
      </div>
    </>
  );
}

const ROW =
  "flex flex-col gap-1 py-3.5 @min-[560px]:grid @min-[560px]:grid-cols-[minmax(8rem,11rem)_4rem_1fr] @min-[560px]:items-center @min-[560px]:gap-x-5";

/** The ruler's numbering, once above the list, aligned to the ruler column. */
function Scale() {
  return (
    <div aria-hidden="true" className={cn(ROW, "py-0")}>
      <div
        className="relative h-4 text-xs text-ink-faint @min-[560px]:col-start-3"
        style={NUMERALS}
      >
        <span className="absolute left-0">0</span>
        <span className="absolute left-1/2 -translate-x-1/2">50</span>
        <span className="absolute right-0">100</span>
      </div>
    </div>
  );
}

/** One area as a ruler row. Pass `drawIn` to sweep the needle in once on mount. */
export function AreaScore({
  drawIn = false,
  className,
  ...props
}: AreaScoreProps & { drawIn?: boolean; className?: string }) {
  const sweep = useSweep(drawIn);
  return (
    <motion.div className={cn("@container", className)} style={{ "--sweep": sweep } as never}>
      <div className={ROW}>
        <ScoreRow {...props} />
      </div>
    </motion.div>
  );
}

/**
 * The list of areas. A ruled list, not a card grid (B4): every needle shares one ruler scale, so
 * the eye reads down the column. With `drawIn`, one clock sweeps every needle in together.
 */
export function AreaScoreList({
  items,
  drawIn = false,
  className,
}: {
  items: AreaScoreProps[];
  drawIn?: boolean;
  className?: string;
}) {
  const sweep = useSweep(drawIn);
  return (
    <motion.div className={cn("@container", className)} style={{ "--sweep": sweep } as never}>
      <Scale />
      <ul className="m-0 flex list-none flex-col p-0">
        {items.map((item) => (
          <li key={item.area} className={ROW}>
            <ScoreRow {...item} />
          </li>
        ))}
      </ul>
    </motion.div>
  );
}

/** The key for the ruler's three marks, drawn with the same shapes. */
export function AreaScoreKey({ className }: { className?: string }) {
  return (
    <ul
      aria-label="Key"
      className={cn(
        "m-0 flex list-none flex-wrap gap-x-5 gap-y-1 p-0 text-sm text-ink-soft",
        className,
      )}
    >
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="h-4 w-[3px] rounded-full bg-ink" />
        Your score
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="mt-3 h-3 w-0.5 rounded-full bg-pencil" />
        Band median
      </li>
      <li className="flex items-center gap-2">
        <span aria-hidden="true" className="relative -mt-1 h-3.5 w-0.5 bg-good">
          <span className="absolute top-0 left-0.5 h-1.5 w-1.5 bg-good [clip-path:polygon(0_0,100%_50%,0_100%)]" />
        </span>
        Next band
      </li>
    </ul>
  );
}

/** The one clock: `--sweep` runs 0 to 1 over `dur.reveal` on `ease.pen`; at rest, and under reduced motion, it is 1 (M10). */
function useSweep(drawIn: boolean): MotionValue<number> {
  const reduced = useReducedMotion();
  const run = drawIn && !reduced;
  const sweep = useMotionValue(run ? 0 : 1);
  useEffect(() => {
    if (!run) {
      sweep.jump(1);
      return;
    }
    sweep.jump(0);
    const controls = animate(sweep, 1, { duration: dur.reveal, ease: ease.pen });
    return () => controls.stop();
  }, [run, sweep]);
  return sweep;
}
