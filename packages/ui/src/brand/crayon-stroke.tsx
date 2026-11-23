"use client";

import { seededRandom } from "../annotations/roughen.js";
import { cn } from "../lib/cn.js";
import { dur, ease, motion, useReducedMotion } from "../motion/index.js";

export interface CrayonStrokeProps {
  /** Seeds the waver, so the same moment always gets the same stroke. */
  seed?: string;
  className?: string;
}

/** A wavering crayon underline in a 100 × 8 box, stretched to its container. */
function crayonPath(seed: string): string {
  const rand = seededRandom(`crayon:${seed}`);
  const y = () => 3.5 + rand() * 2;
  // Four bows across the width, the last one lifting a little as a hand does at the end of a line.
  const points = [
    [2, y()],
    [27, y()],
    [52, y()],
    [77, y()],
    [98, 2.5 + rand()],
  ] as const;
  let d = `M ${points[0][0]} ${points[0][1].toFixed(2)}`;
  for (let i = 1; i < points.length; i++) {
    const [x0, y0] = points[i - 1] ?? points[0];
    const [x1, y1] = points[i] ?? points[0];
    const cy = (y0 + y1) / 2 + (rand() - 0.5) * 3;
    d += ` Q ${((x0 + x1) / 2).toFixed(2)} ${cy.toFixed(2)} ${x1} ${y1.toFixed(2)}`;
  }
  return d;
}

/**
 * The crayon under a celebration (a drill solved, a milestone). Decorative: the words beside it say
 * what happened. It draws once on the pen easing, and is simply there under reduced motion (M10).
 */
export function CrayonStroke({ seed = "crayon", className }: CrayonStrokeProps) {
  const reduced = useReducedMotion();
  // The reveal is a left-to-right clip, not pathLength: dashes are measured on screen under
  // non-scaling-stroke, so a pathLength of 1 would cut the stretched line short.
  return (
    <motion.svg
      data-crayon-stroke
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 100 8"
      preserveAspectRatio="none"
      className={cn("block h-2 w-full overflow-visible", className)}
      initial={reduced ? false : { clipPath: "inset(-4px 100% -4px -4px)" }}
      animate={{ clipPath: "inset(-4px -4px -4px -4px)" }}
      transition={{ duration: dur.draw * 2, ease: ease.pen }}
    >
      {/* Two passes, as a crayon goes back over its line: a broad one, then a thinner one beside it. */}
      <path
        d={crayonPath(seed)}
        fill="none"
        stroke="var(--color-crayon)"
        strokeWidth={5}
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
      <path
        d={crayonPath(`${seed}:again`)}
        fill="none"
        stroke="var(--color-crayon)"
        strokeWidth={2.5}
        strokeLinecap="round"
        strokeOpacity={0.75}
        vectorEffect="non-scaling-stroke"
      />
    </motion.svg>
  );
}
