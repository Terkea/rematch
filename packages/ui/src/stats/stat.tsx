"use client";

import NumberFlow, { type Format } from "@number-flow/react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/index.js";
import { dur, ease, useReducedMotion } from "../motion/index.js";
import { DEFAULT_LOCALE, type StatFormat } from "./band.js";

export { DEFAULT_LOCALE, type StatFormat };

/** The headline value: percent to 0.1, decimal to 0.01, integer whole. */
export const valueFormat: Record<StatFormat, Format> = {
  percent: { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 },
  decimal: { minimumFractionDigits: 2, maximumFractionDigits: 2 },
  integer: { maximumFractionDigits: 0 },
};

export interface StatProps {
  value: number;
  format: StatFormat;
  locale?: string;
  /**
   * First-view roll (DESIGN.md choreography 2): when the nearest `[data-reveal-fig]` ancestor is
   * still transparent on mount (its reveal hasn't shown it yet), the number drops to zero unseen
   * and rolls up to its value after `rollDelay` ms. Already visible, it never moves.
   */
  rollIn?: boolean;
  rollDelay?: number;
  className?: string;
}

const PEN = `cubic-bezier(${ease.pen.join(",")})`;
/** NumberFlow's digit roll on the app's motion tokens: `dur.reveal` on the pen ease. */
const TIMING = {
  transformTiming: { duration: dur.reveal * 1000, easing: PEN },
  spinTiming: { duration: dur.reveal * 1000, easing: PEN },
  opacityTiming: { duration: dur.quick * 1000, easing: "ease-out" },
};

/**
 * A number that rolls to its new value (NumberFlow) in tabular lining figures, so digits never
 * jitter sideways. Under reduced motion it swaps instantly (M10).
 */
export function Stat({
  value,
  format,
  locale = DEFAULT_LOCALE,
  rollIn = false,
  rollDelay = 0,
  className,
}: StatProps) {
  const reduced = useReducedMotion();
  const ref = useRef<HTMLSpanElement | null>(null);
  // `from` is the value shown while the roll waits; null shows the real value.
  const [from, setFrom] = useState<number | null>(null);
  useEffect(() => {
    if (!rollIn || reduced) return;
    const host = ref.current?.closest("[data-reveal-fig]");
    if (!host || Number.parseFloat(getComputedStyle(host).opacity) > 0.05) return;
    setFrom(0);
    const t = setTimeout(() => setFrom(null), rollDelay);
    return () => {
      clearTimeout(t);
      setFrom(null);
    };
  }, [rollIn, reduced, rollDelay]);
  return (
    <NumberFlow
      ref={ref as never}
      value={from ?? value}
      format={valueFormat[format]}
      locales={locale}
      animated={!reduced && from === null}
      {...TIMING}
      className={cn("[font-variant-numeric:lining-nums_tabular-nums]", className)}
    />
  );
}
