// The weekly focus's copy as plain functions (no "use client"), so server components and the
// client controls share it. Every helper guards its numbers: never NaN, Infinity or undefined.

import type { FocusOutcome, Progress, Target, Verdict } from "../../server/focus/goal";
import type { TimeClass } from "../../server/games/types";
import { ESTIMATE_WINDOW_TEXT } from "../../server/profile/window";
import type { Interval } from "../../server/stats/wilson";

const fin = (v: number | null | undefined): v is number =>
  typeof v === "number" && Number.isFinite(v);
const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

/** "15.0%", or a dash when the share is unknown. */
export const pctText = (share: number | null | undefined) =>
  fin(share) ? `${(share * 100).toFixed(1)}%` : "–";

const plural = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;

const rangeText = (i: Interval | null) =>
  i && fin(i.low) && fin(i.high) ? ` (80% range ${pctText(i.low)} to ${pctText(i.high)})` : "";

/** "Get it down to 15.0% of your chances over your next 20 rapid games, from 20.0% now." */
export function goalText(g: {
  targetRate: number;
  baselineRate: number;
  gamesN: number;
  timeClass: TimeClass;
}): string {
  return `Get it down to ${pctText(g.targetRate)} of your chances over your next ${count(g.gamesN)} ${g.timeClass} games, from ${pctText(g.baselineRate)} now.`;
}

/** The current rate with its n, and where the target came from. */
export function targetWhyText(s: {
  target: Target;
  baseline: { rate: number; events: number; chances: number };
}): string {
  const now = `Now ${pctText(s.baseline.rate)}, from ${count(s.baseline.events)} of ${plural(s.baseline.chances, "chance", "chances")} ${ESTIMATE_WINDOW_TEXT}.`;
  const why =
    s.target.basis === "peer"
      ? "The target is the rate of the players you're compared with."
      : "The target is a quarter fewer.";
  return `${now} ${why}`;
}

/** "So far 1 of 12 chances, 8.3% (80% range 3.0% to 20.1%)." */
export function progressText(p: Progress): string {
  if (p.chances <= 0 || p.rate === null) return "No chances yet in these games.";
  return `So far ${count(p.events)} of ${plural(p.chances, "chance", "chances")}, ${pctText(p.rate)}${rangeText(p.interval)}.`;
}

/** "9 of 20 games". */
export const gamesText = (p: Progress) => `${count(p.games.played)} of ${count(p.games.of)} games`;

export function verdictText(v: Verdict): string {
  switch (v.kind) {
    case "on_track":
      return "On track";
    case "not_yet":
      return "Not yet";
    case "too_early":
      return `Too early to tell: ${plural(v.moreChances, "more chance", "more chances")}`;
  }
}

export function outcomeText(o: FocusOutcome): string {
  switch (o) {
    case "met":
      return "Goal met";
    case "not_met":
      return "Goal not met";
    case "inconclusive":
      return "Too few chances to tell";
  }
}

/** The weekly review's "Your focus this week" line. */
export function weekLineText(l: {
  label: string;
  events: number;
  chances: number;
  games: number;
  targetRate: number;
  rate: number | null;
  interval: Interval | null;
}): string {
  const games = plural(l.games, "game", "games");
  if (l.chances <= 0 || l.rate === null) {
    return `${l.label}: no chances in ${games} this week. The target is ${pctText(l.targetRate)}.`;
  }
  return `${l.label}: ${count(l.events)} of ${plural(l.chances, "chance", "chances")} in ${games}, ${pctText(l.rate)}${rangeText(l.interval)}, against a target of ${pctText(l.targetRate)}.`;
}
