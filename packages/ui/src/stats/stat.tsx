"use client";

import NumberFlow, { type Format } from "@number-flow/react";
import { cn } from "../lib/index.js";
import { dur, ease, useReducedMotion } from "../motion/index.js";

export type StatFormat = "percent" | "decimal" | "integer";

/** Stats render in one locale on server and client, so hydration never disagrees. */
export const DEFAULT_LOCALE = "en-US";

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
export function Stat({ value, format, locale = DEFAULT_LOCALE, className }: StatProps) {
  const reduced = useReducedMotion();
  return (
    <NumberFlow
      value={value}
      format={valueFormat[format]}
      locales={locale}
      animated={!reduced}
      {...TIMING}
      className={cn("[font-variant-numeric:lining-nums_tabular-nums]", className)}
    />
  );
}
