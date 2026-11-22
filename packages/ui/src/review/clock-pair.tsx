"use client";

import { cn } from "../lib/index.js";
import { motion, spring, useReducedMotion } from "../motion/index.js";
import { clockDuration, formatClock, formatSpent } from "./clock.js";

export interface ClockSide {
  name: string;
  rating: number | null;
  /** The side's clock at the shown ply, or null when the game has no clock data. */
  seconds: number | null;
  toMove: boolean;
}

export interface ClockPairProps {
  /** The side at the top of the board. */
  top: ClockSide;
  /** The side at the bottom of the board. */
  bottom: ClockSide;
  /** Time the mover spent on the shown move, or null when it isn't known. */
  spent: { seconds: number } | null;
  className?: string;
}

const NUMERALS = "[font-variant-numeric:lining-nums_tabular-nums]";

function SideRow({
  side,
  at,
  reduced,
}: {
  side: ClockSide;
  at: "top" | "bottom";
  reduced: boolean;
}) {
  const text = side.seconds === null ? "" : formatClock(side.seconds);
  const low = text !== "" && side.seconds !== null && side.seconds < 20;
  return (
    <div data-side={at} className="flex min-w-0 items-center gap-3">
      <span aria-hidden="true" className="relative h-5 w-1 shrink-0">
        {side.toMove ? (
          // The to-move bar: drawn down from its top, like a stroke of the pen.
          <motion.span
            data-to-move-bar
            className="absolute inset-0 origin-top rounded-pill bg-ink"
            initial={reduced ? false : { scaleY: 0 }}
            animate={{ scaleY: 1 }}
            transition={spring.land}
          />
        ) : null}
      </span>
      <span className="flex min-w-0 flex-1 items-baseline gap-2">
        <span
          className={cn(
            "truncate font-sans text-base font-medium",
            side.toMove ? "text-ink" : "text-ink-soft",
          )}
        >
          {side.name}
        </span>
        {side.rating !== null && Number.isFinite(side.rating) ? (
          <span className={cn("shrink-0 font-sans text-sm text-ink-faint", NUMERALS)}>
            {side.rating}
          </span>
        ) : null}
        {side.toMove ? <span className="sr-only">, to move</span> : null}
      </span>
      {text ? (
        <time
          dateTime={clockDuration(side.seconds ?? 0)}
          data-low={low || undefined}
          className={cn(
            "shrink-0 rounded-field px-2 py-0.5 font-serif text-xl font-semibold whitespace-nowrap",
            NUMERALS,
            "transition-colors duration-(--dur-quick) ease-out",
            side.toMove ? "bg-ink text-on-ink" : "bg-well text-ink-soft",
          )}
        >
          {text}
        </time>
      ) : null}
    </div>
  );
}

/**
 * Both players with their clocks at the shown ply. The side to move has an `ink` bar beside its
 * name and its clock face inked, like the running side of a chess clock. A side without clock data
 * shows no clock at all, never 0:00. Below 20 s the clock shows tenths.
 */
export function ClockPair({ top, bottom, spent, className }: ClockPairProps) {
  const reduced = useReducedMotion();
  const spentText = spent ? formatSpent(spent.seconds) : null;
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <SideRow side={top} at="top" reduced={reduced} />
      {spentText ? (
        <p className="pl-4 font-sans text-sm text-ink-soft [font-variant-numeric:lining-nums]">
          {spentText}
        </p>
      ) : null}
      <SideRow side={bottom} at="bottom" reduced={reduced} />
    </div>
  );
}
