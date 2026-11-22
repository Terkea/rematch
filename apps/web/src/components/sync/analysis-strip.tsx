"use client";

import { cn } from "@ci/ui/lib";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { useEffect, useRef } from "react";
import type { AnalysisCounts } from "../../server/sync";
import { analysisTotal } from "./copy";

/** A tick's state; each has its own shape, not only its own colour. */
export type Tick = "done" | "analysing" | "failed" | "waiting";

/**
 * Splits the picked games into at most `max` ticks, in the order done, analysing (with
 * retrying), failed, waiting, by largest remainder. A state with any games keeps at least one
 * tick, so 1 failed game of 900 is still visible. Empty for no games.
 */
export function allocateTicks(c: AnalysisCounts, max: number): Tick[] {
  const parts: [Tick, number][] = [
    ["done", c.done],
    ["analysing", c.analysing + c.retrying],
    ["failed", c.failed],
    ["waiting", c.waiting],
  ];
  const total = analysisTotal(c);
  if (total <= 0 || max <= 0) return [];
  const n = Math.min(total, max);
  const shares = parts.map(([tick, count]) => {
    const exact = (count / total) * n;
    const base = count > 0 ? Math.max(1, Math.floor(exact)) : 0;
    return { tick, count, base, rest: exact - Math.floor(exact) };
  });
  let left = n - shares.reduce((s, p) => s + p.base, 0);
  // Hand out what is left by remainder; take back from the largest share if the minimums overshot.
  const byRest = [...shares].filter((p) => p.count > 0).sort((a, b) => b.rest - a.rest);
  for (let i = 0; left > 0 && byRest.length > 0; i = (i + 1) % byRest.length, left--) {
    const p = byRest[i];
    if (p) p.base++;
  }
  while (left < 0) {
    const big = shares.reduce((a, b) => (b.base > a.base ? b : a));
    big.base--;
    left++;
  }
  return shares.flatMap((p) => Array.from({ length: p.base }, () => p.tick));
}

/** The strip's accessible name: the counts in words. */
export function stripLabel(c: AnalysisCounts): string {
  const parts = [`${c.done} of ${analysisTotal(c)} analysed`];
  if (c.analysing + c.retrying > 0) parts.push(`${c.analysing + c.retrying} analysing`);
  if (c.waiting > 0) parts.push(`${c.waiting} waiting`);
  if (c.failed > 0) parts.push(`${c.failed} failed`);
  return parts.join(", ");
}

const TICK = "block w-0.5 shrink-0 rounded-pill";

/** One tick: done is a full ink stroke, analysing a dashed pencil stroke, failed a broken stroke, waiting a stub. */
function TickMark({ tick, animate }: { tick: Tick; animate: boolean }) {
  const reduced = useReducedMotion();
  if (tick === "waiting") return <span className={cn(TICK, "h-1 self-end bg-rule-strong")} />;
  if (tick === "failed") {
    return (
      <span className="flex h-full w-0.5 shrink-0 flex-col justify-between">
        <span className={cn(TICK, "h-1 bg-ink-soft")} />
        <span className={cn(TICK, "h-1 bg-ink-soft")} />
      </span>
    );
  }
  if (tick === "analysing") {
    return <span className="block h-full w-0.5 shrink-0 border-l-2 border-dashed border-pencil" />;
  }
  // A game just finished: its stroke is written top to bottom, like a pen mark (M1, M9).
  const initial = !animate ? false : reduced ? { opacity: 0 } : { scaleY: 0 };
  return (
    <motion.span
      data-tick="done"
      className={cn(TICK, "h-full origin-top bg-ink")}
      initial={initial}
      animate={{ scaleY: 1, opacity: 1 }}
      transition={{ duration: reduced ? dur.quick : dur.drawSmall, ease: ease.pen }}
    />
  );
}

/**
 * Where an account's analysis stands, as a tally of ticks (one per game, or per group of games
 * past `max`). The sentence beside it carries the exact counts; this is its picture, named for
 * assistive tech. Nothing animates on the first render; a tick that turns done draws in.
 */
export function AnalysisStrip({
  counts,
  max,
  className,
}: {
  counts: AnalysisCounts;
  max: number;
  className?: string;
}) {
  const ticks = allocateTicks(counts, max);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
  }, []);
  if (ticks.length === 0) return null;
  return (
    <div
      role="img"
      aria-label={stripLabel(counts)}
      data-part="analysis-strip"
      className={cn("flex h-3 w-full items-stretch justify-between gap-px", className)}
      // A fixed pitch per tick (2 spacing units), so a small sample draws a short tally.
      style={{ maxWidth: `calc(${ticks.length} * var(--spacing) * 2)` }}
    >
      {ticks.map((tick, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a tick is its place; a new state remounts it.
        <TickMark key={`${i}-${tick}`} tick={tick} animate={mounted.current} />
      ))}
    </div>
  );
}
