"use client";

import { useId } from "react";
import { fnv1a } from "../annotations/roughen.js";
import { cn } from "../lib/cn.js";

export interface PencilTextureProps {
  /** Seeds the grain, so a surface always gets the same texture. */
  seed?: string;
  className?: string;
}

/**
 * A faint graphite grain for empty-state and landing illustrations: seeded SVG turbulence in
 * `pencil` at 5% opacity. Decorative, and never under text or data (DESIGN.md, "Hand-made touches").
 * The host positions it (it fills its nearest positioned ancestor).
 */
export function PencilTexture({ seed = "pencil", className }: PencilTextureProps) {
  const id = `pencil-${useId().replace(/:/g, "")}`;
  return (
    <svg
      data-pencil-texture
      aria-hidden="true"
      focusable="false"
      opacity={0.05}
      className={cn("pointer-events-none absolute inset-0 h-full w-full", className)}
    >
      <filter id={id} x="0" y="0" width="100%" height="100%">
        <feTurbulence
          type="fractalNoise"
          baseFrequency="0.85"
          numOctaves={2}
          seed={fnv1a(seed) % 1000}
          stitchTiles="stitch"
          result="noise"
        />
        <feColorMatrix in="noise" type="luminanceToAlpha" result="grain" />
        <feFlood style={{ floodColor: "var(--color-pencil)" }} result="graphite" />
        <feComposite in="graphite" in2="grain" operator="in" />
      </filter>
      <rect width="100%" height="100%" filter={`url(#${id})`} />
    </svg>
  );
}
