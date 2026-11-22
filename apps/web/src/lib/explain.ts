import { NO_BENCHMARK_TEXT } from "@ci/ui/profile";
import { DETECTOR_CONSTANTS as K } from "../server/profile/glossary";
import type { WeaknessState } from "../server/profile/types";
import type { Estimate } from "../server/stats";

// "What this means for you": one plain line per headline, built only from the numbers on the page
// (value, interval, n and whether a benchmark exists). No filler: each line either translates a
// number into what it means in a game, or says exactly why it cannot yet (honesty rules: n with
// every rate, unknown is never 0, no NaN, nothing called better or worse unless ranges separate).

const fin = (v: number | null | undefined): v is number =>
  typeof v === "number" && Number.isFinite(v);
const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);
/** One decimal, with a whole number written bare ("3", "2.6"). */
const one = (v: number) => String(Number(v.toFixed(1)));
const NOT_COMPARED = "Not compared with players at your level yet.";
const DROP = `${K.decisiveDropWinPct} win% points or more`;

/** "about 1 in 3", "about 8 in 10", "every time", "never"; null when there is nothing to count. */
export function oneInText(events: number, chances: number): string | null {
  if (!fin(events) || !fin(chances) || chances <= 0 || events < 0) return null;
  if (events === 0) return "never";
  if (events >= chances) return "every time";
  const r = events / chances;
  if (r >= 0.5) return `about ${Math.round(r * 10)} in 10`;
  return `about 1 in ${Math.round(1 / r)}`;
}

/** "In the last 6 months this happened in 14 of your 40 chances (about 1 in 3)." */
export function howOftenText(c: { events: number; chances: number }, window: string): string {
  const head = window.charAt(0).toUpperCase() + window.slice(1);
  if (!fin(c.chances) || c.chances <= 0) {
    return `${head} you had no chances for this yet, so there is nothing to count.`;
  }
  const chances = `${count(c.chances)} ${c.chances === 1 ? "chance" : "chances"}`;
  if (!fin(c.events) || c.events <= 0) return `${head} this happened in none of your ${chances}.`;
  return `${head} this happened in ${count(c.events)} of your ${chances} (${oneInText(c.events, c.chances)}).`;
}

export type TrendInput = {
  state: WeaknessState;
  /** Whether a rating-band benchmark exists: without one the stored state is no claim. */
  compared: boolean;
  sufficient: boolean;
  needMore: number | null;
  /** Rust `p_imp`: the chance the rate is at least 15% lower than before its last change. */
  pImp: number | null;
};

const pct = (p: number) => `${Math.round(Math.min(1, Math.max(0, p)) * 100)}%`;

/** Better or worse, in plain words, from the stored state and `p_imp` only. */
export function trendText(t: TrendInput): string {
  if (!t.sufficient) {
    if (t.needMore === null || !fin(t.needMore) || t.needMore <= 0) {
      return "Too early to tell. There are not enough chances yet to call it better or worse.";
    }
    const more = t.needMore === 1 ? "1 more chance is" : `${count(t.needMore)} more chances are`;
    return `Too early to tell. ${more} needed before it can be called better or worse.`;
  }
  if (!t.compared) {
    return "No verdict yet. Without a benchmark for players at your level, it is not tracked as getting better or worse.";
  }
  const p = fin(t.pImp) ? t.pImp : null;
  switch (t.state) {
    case "improving":
      return p !== null
        ? `Getting better. Your rate has fallen since its last change, and we are ${pct(p)} sure it is lower now. It is not settled yet.`
        : "Getting better. A fall in your rate has been confirmed. It is not settled yet.";
    case "emerging":
      return "Getting worse. A rise in your rate has been confirmed recently.";
    case "relapsed":
      return "Coming back. It had been fixed, and your rate has been drifting up again.";
    case "resolved":
      return `Fixed for now. Your rate stayed low for ${K.resolveStreakGames} analysed games in a row.`;
    case "active":
      return p !== null
        ? `No clear change yet. There is a ${pct(p)} chance your rate is lower than before its last change, short of the ${K.improvingChancePct}% needed to call it better.`
        : "No clear change. No rise or fall in your rate has been confirmed.";
    default:
      return "Steady. No rise or fall in your rate has been confirmed.";
  }
}

/** "Your win chance went from 52% to 36%, a drop of 16 win% points."; null without both values. */
export function exampleLine(before: number, after: number): string | null {
  if (!fin(before) || !fin(after)) return null;
  const clamp = (v: number) => Math.round(Math.min(100, Math.max(0, v)));
  const b = clamp(before);
  const a = clamp(after);
  if (a === b) return `Your win chance stayed at ${b}%.`;
  return a < b
    ? `Your win chance went from ${b}% to ${a}%, a drop of ${b - a} win% points.`
    : `Your win chance went from ${b}% to ${a}%, a rise of ${a - b} win% points.`;
}

/** The overview headline in a game's terms: how often a winning chance is given away. */
export function headlineMeaning(h: {
  accuracy: Estimate;
  blunders: Estimate;
  /** "rapid games", "analysed games". */
  games: string;
}): string {
  const { accuracy: acc, blunders: bl } = h;
  if (acc.kind === "insufficient") {
    return `Too little evidence yet: with ${count(acc.n)} ${h.games}, one unusual game moves the average a lot.`;
  }
  if (bl.kind === "estimate" && fin(bl.value)) {
    const v = bl.value;
    const range =
      bl.interval && fin(bl.interval.low) && fin(bl.interval.high)
        ? { low: one(bl.interval.low), high: one(bl.interval.high) }
        : null;
    if (v <= 0) {
      return `None of your ${count(bl.n)} ${h.games} had a blunder (a move that costs ${DROP}). ${NOT_COMPARED}`;
    }
    if (v < 0.95) {
      const every = Math.round(1 / v);
      const r = range ? `, likely between ${range.low} and ${range.high} a game` : "";
      return `You make a blunder (a move that costs ${DROP}) about once every ${every} games${r}. ${NOT_COMPARED}`;
    }
    const what =
      v < 1.05 ? "one blunder (a move that costs" : `${one(v)} blunders (moves that cost`;
    const r = range ? `, likely between ${range.low} and ${range.high}` : "";
    return `In a typical game you make about ${what} ${DROP})${r}. ${NOT_COMPARED}`;
  }
  if (acc.interval && fin(acc.interval.low) && fin(acc.interval.high)) {
    return `Your accuracy is likely between ${one(acc.interval.low)}% and ${one(acc.interval.high)}% across ${count(acc.n)} ${h.games}. ${NOT_COMPARED}`;
  }
  return `Your accuracy is ${one(acc.value)}% across ${count(acc.n)} ${h.games}, the same in every game, so it has no range. ${NOT_COMPARED}`;
}

const SCALE = "on a scale where 50 is the typical player in your rating band";
const NO_RANGE = "It has no range yet, so read it as a lean, not a verdict.";

/** An area score (a percentile-like 0 to 100, 50 typical) in words, or why there is none. */
export function areaMeaning(a: {
  score: number | null;
  interval: { low: number; high: number } | null;
  /** The thinnest measure's sample. */
  n: number;
  /** Mindset: not scored by design. */
  unscored?: boolean;
}): string {
  if (a.unscored) {
    return "Not scored: session habits do not compare cleanly with other players, so the measures below describe it instead.";
  }
  if (!fin(a.score)) {
    return fin(a.n) && a.n > 0
      ? `Too little evidence for a score yet: its thinnest measure has ${count(a.n)} so far.`
      : "Too little evidence for a score yet: none of its measures has a sample.";
  }
  const s = Math.round(a.score);
  const head = `${s} ${SCALE}`;
  if (a.interval && fin(a.interval.low) && fin(a.interval.high)) {
    const lo = Math.floor(a.interval.low);
    const hi = Math.ceil(a.interval.high);
    const verdict =
      lo > 50
        ? "above typical"
        : hi < 50
          ? "below typical"
          : "it cannot yet be told apart from typical";
    return `${head}: likely between ${lo} and ${hi}, so ${verdict}.`;
  }
  const gap = s - 50;
  const where =
    gap === 0
      ? "level with typical"
      : `${Math.abs(gap)} ${Math.abs(gap) === 1 ? "point" : "points"} ${gap < 0 ? "below" : "above"} typical`;
  return `${head}: ${where}. ${NO_RANGE}`;
}

/** You against the rating band's median: only a median outside the 80% range says more or less. */
export function levelText(l: {
  rate: { mean: number; low: number; high: number };
  median: number | null;
  compared: boolean;
  sufficient: boolean;
}): string {
  if (!l.compared || !fin(l.median)) return `${NO_BENCHMARK_TEXT}.`;
  const m = `${(l.median * 100).toFixed(1)}%`;
  if (!l.sufficient)
    return `Typical players at your level: ${m}. A comparison waits for more chances.`;
  if (l.median < l.rate.low) {
    return `More often than typical players at your level (${m}): their rate is below your whole likely range.`;
  }
  if (l.median > l.rate.high) {
    return `Less often than typical players at your level (${m}): their rate is above your whole likely range.`;
  }
  return `Not clearly different from typical players at your level (${m}): their rate is inside your likely range.`;
}

/** "Your estimated rate is 4.2% of chances, likely between 3.1% and 5.7% (80% range)." */
export function rateText(r: { mean: number; low: number; high: number }, early: boolean): string {
  if (!fin(r.mean) || !fin(r.low) || !fin(r.high)) return "Your rate is not estimated yet.";
  const p = (v: number) => `${(v * 100).toFixed(1)}%`;
  return `Your ${early ? "early estimate" : "estimated rate"} is ${p(r.mean)} of chances, likely between ${p(r.low)} and ${p(r.high)} (80% range).`;
}
