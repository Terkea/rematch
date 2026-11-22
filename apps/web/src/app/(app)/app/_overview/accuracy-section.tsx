"use client";

import { AccuracyTrend } from "@ci/ui/charts";
import { Button } from "@ci/ui/primitives";
import { useState, useSyncExternalStore } from "react";
import { TREND_WINDOW } from "../../../../server/stats/constants";
import { type AccuracyView, TREND_GAP_DAYS } from "./view";

/** Phones show the latest 100 games by default, with a way to all of them (T4 review ruling). */
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

/** The latest `n` games (points are oldest first) and the rolling entries that end on them. */
function latest(view: AccuracyView, n: number): Pick<AccuracyView, "points" | "rolling"> {
  const points = view.points.slice(Math.max(0, view.points.length - n));
  const ids = new Set(points.map((p) => p.id));
  return { points, rolling: view.rolling.filter((r) => ids.has(r.id)) };
}

/**
 * The accuracy trend and its caption. A client island: it gets the narrow `AccuracyView` only
 * (ids, dates, accuracies, results, captions without opponents, review links).
 */
export function AccuracySection({ accuracy, drawIn }: { accuracy: AccuracyView; drawIn: boolean }) {
  const narrow = useSyncExternalStore(subscribe, snapshot, () => false);
  const [all, setAll] = useState(false);
  const total = accuracy.points.length;
  const long = narrow && total > PHONE_GAMES;
  const windowed = long && !all;
  const shown = windowed ? latest(accuracy, PHONE_GAMES) : accuracy;
  const noLine = accuracy.rolling.length === 0;
  return (
    <div className="flex flex-col gap-3">
      <div data-chart="trend">
        <AccuracyTrend
          points={shown.points}
          rolling={shown.rolling}
          label={
            windowed ? `Accuracy over time, latest ${PHONE_GAMES} games` : "Accuracy over time"
          }
          gapDays={TREND_GAP_DAYS}
          minPoints={1}
          {...(drawIn ? { drawIn } : {})}
        />
      </div>
      {long && (
        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2 text-sm text-ink-soft">
          <span>
            {windowed
              ? `Showing your latest ${PHONE_GAMES} of ${total} games.`
              : `Showing all ${total} games.`}
          </span>
          <Button variant="outline" size="sm" onClick={() => setAll((v) => !v)}>
            {windowed ? `Show all ${total}` : `Show the latest ${PHONE_GAMES}`}
          </Button>
        </div>
      )}
      <div className="flex max-w-prose flex-col gap-1 text-sm text-ink-soft">
        <p>
          {`Each dot is a game. The line is your average over the last ${TREND_WINDOW} games, with an 80% band.`}
        </p>
        <p>
          {noLine
            ? `The line starts once you have ${TREND_WINDOW} analysed games.`
            : "The line breaks where four weeks or more passed with no games."}
        </p>
        {accuracy.capped && <p>Showing your latest 500 analysed games.</p>}
      </div>
    </div>
  );
}
