"use client";

import { AccuracyTrend, type AccuracyTrendPoint, type AccuracyTrendRolling } from "@ci/ui/charts";
import { Button } from "@ci/ui/primitives";
import { useState, useSyncExternalStore } from "react";
import { latest } from "./view";

/** Phones show the last 100 games by default (controller ruling, T4 review), with a way to all. */
export const PHONE_GAMES = 100;
const NARROW = "(max-width: 39.99rem)";

function subscribe(onChange: () => void) {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  const m = window.matchMedia(NARROW);
  m.addEventListener("change", onChange);
  return () => m.removeEventListener("change", onChange);
}
const snapshot = () =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(NARROW).matches
    : false;

export interface TrendWindowProps {
  points: AccuracyTrendPoint[];
  rolling: AccuracyTrendRolling[];
  label: string;
  drawIn?: boolean;
  className?: string;
}

/** The accuracy trend, narrowed to the latest games on a phone so the dots stay readable. */
export function TrendWindow({ points, rolling, label, drawIn, className }: TrendWindowProps) {
  const narrow = useSyncExternalStore(subscribe, snapshot, () => false);
  const [all, setAll] = useState(false);
  const windowed = narrow && !all && points.length > PHONE_GAMES;
  const shown = windowed ? latest(points, rolling, PHONE_GAMES) : { points, rolling };
  return (
    <div className={className}>
      <AccuracyTrend
        points={shown.points}
        rolling={shown.rolling}
        label={windowed ? `${label}, latest ${PHONE_GAMES} games` : label}
        {...(drawIn ? { drawIn } : {})}
      />
      {narrow && points.length > PHONE_GAMES && (
        <p className="mt-2 flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-sm text-ink-soft">
          <span>
            {windowed
              ? `Showing your latest ${PHONE_GAMES} of ${points.length} games.`
              : `Showing all ${points.length} games.`}
          </span>
          <Button variant="quiet" size="sm" onClick={() => setAll((v) => !v)}>
            {windowed ? `Show all ${points.length}` : `Show the latest ${PHONE_GAMES}`}
          </Button>
        </p>
      )}
    </div>
  );
}
