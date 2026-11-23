import { type Interval, wilson } from "../stats/wilson";

// The weekly focus's arithmetic (ADR 0090): how its target is chosen and how its progress is
// judged. Pure, so the rules are tested apart from the database.

/** The default length of a focus, in analysed games. */
export const DEFAULT_GAMES_N = 20;
/** The lengths a player can pick. */
export const GAMES_N_CHOICES = [10, 20, 30, 40] as const;
export const MIN_GAMES_N = 5;
export const MAX_GAMES_N = 100;

/** The relative step down from the current rate: a quarter fewer. */
export const RELATIVE_STEP = 0.25;
/** A peer rate counts as a target only when it is at least this far below the current rate. */
export const MIN_PEER_STEP = 0.1;
/** Chances before the progress gets a verdict, and before a closed focus is met or not met. */
export const MIN_VERDICT_CHANCES = 10;

export type Target = { rate: number; basis: "peer" | "relative" };

const fin = (v: number | null | undefined): v is number =>
  typeof v === "number" && Number.isFinite(v);
/** To a tenth of a percentage point; the epsilon keeps 92.55 from flooring to 92. */
const tenth = (v: number) => Math.round(v * 1000 + 1e-9) / 1000;

/**
 * The goal's rate: the current rate less a meaningful step, the peer median or a quarter
 * fewer, whichever is easier (the higher rate). A peer median counts only when it is at least
 * `MIN_PEER_STEP` below the current rate. Null when the rate is unknown or already 0.
 */
export function chooseTarget(t: { baseline: number; peer: number | null }): Target | null {
  if (!fin(t.baseline) || t.baseline <= 0 || t.baseline > 1) return null;
  const relative = tenth(t.baseline * (1 - RELATIVE_STEP));
  const peer =
    fin(t.peer) && t.peer >= 0 && t.peer <= t.baseline * (1 - MIN_PEER_STEP) ? tenth(t.peer) : null;
  return peer !== null && peer > relative
    ? { rate: peer, basis: "peer" }
    : { rate: relative, basis: "relative" };
}

export type Verdict =
  | { kind: "on_track" }
  | { kind: "not_yet" }
  | { kind: "too_early"; moreChances: number };

export type FocusOutcome = "met" | "not_met" | "inconclusive";

export type Progress = {
  events: number;
  chances: number;
  /** The share of chances so far; null with no chance yet. */
  rate: number | null;
  /** Its 80% Wilson interval; null with no chance yet. */
  interval: Interval | null;
  games: { played: number; of: number };
  verdict: Verdict;
  /** The focus has reached its N games. */
  done: boolean;
  outcome: FocusOutcome | null;
};

/**
 * Progress against the goal from the focus's own games: k of n chances, the rate with its 80%
 * interval and the games played out of N. Below `MIN_VERDICT_CHANCES` it is too early to tell,
 * and a focus that closes there is inconclusive; otherwise the rate is held against the target.
 */
export function focusProgress(p: {
  k: number;
  n: number;
  games: number;
  gamesN: number;
  target: number;
}): Progress {
  const n = Math.max(0, Math.trunc(p.n));
  const k = Math.min(Math.max(0, Math.trunc(p.k)), n);
  const rate = n > 0 ? k / n : null;
  const interval = wilson(k, n, 0.8);
  const played = Math.min(Math.max(0, Math.trunc(p.games)), p.gamesN);
  const done = played >= p.gamesN;
  const enough = n >= MIN_VERDICT_CHANCES && rate !== null;
  const below = enough && rate <= p.target;
  const verdict: Verdict = !enough
    ? { kind: "too_early", moreChances: MIN_VERDICT_CHANCES - n }
    : below
      ? { kind: "on_track" }
      : { kind: "not_yet" };
  const outcome: FocusOutcome | null = !done
    ? null
    : !enough
      ? "inconclusive"
      : below
        ? "met"
        : "not_met";
  return {
    events: k,
    chances: n,
    rate,
    interval,
    games: { played, of: p.gamesN },
    verdict,
    done,
    outcome,
  };
}
