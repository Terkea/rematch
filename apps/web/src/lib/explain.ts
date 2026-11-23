import { likelyText, NO_BENCHMARK_TEXT, probablyAbove } from "@ci/ui/profile";
import { stateStyle } from "@ci/ui/timeline";
import { BENCHMARK_BIAS_TEXT, type Benchmark } from "../server/profile/benchmark-text";
import { DETECTOR_CONSTANTS as K, VERDICT_CONSTANTS as V } from "../server/profile/glossary";
import type { MateChange, WeaknessState } from "../server/profile/types";
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

/** An engine score from White's point of view, as stored (`{ cp }` or `{ mate }`). */
export type WhiteScore = { cp?: number | null; mate?: number | null };

/**
 * The `MateChange` of a move (see its type). Scores are White's point of view; the mover is the
 * side to move in `fen`. Null when neither happened or a score is unknown.
 */
export function mateChange(
  fen: string,
  before: WhiteScore | null | undefined,
  after: WhiteScore | null | undefined,
): MateChange | null {
  if (!before || !after) return null;
  const sign = fen.split(" ")[1] === "b" ? -1 : 1;
  const mate = (s: WhiteScore) => (fin(s.mate) && s.mate !== 0 ? s.mate * sign : null);
  const b = mate(before);
  const a = mate(after);
  if (b !== null && b > 0 && !(a !== null && a > 0)) return { kind: "lost", n: b };
  if (a !== null && a < 0 && !(b !== null && b < 0)) return { kind: "allowed", n: -a };
  return null;
}

/**
 * "Your win chance went from 52% to 36%, a drop of 16 win% points."; null without both values.
 * A mate the move gave up or allowed is said first: it is why the move was judged, whatever the
 * drop (critic 1).
 */
export function exampleLine(
  before: number,
  after: number,
  mate: MateChange | null = null,
): string | null {
  if (!fin(before) || !fin(after)) return null;
  const clamp = (v: number) => Math.round(Math.min(100, Math.max(0, v)));
  const b = clamp(before);
  const a = clamp(after);
  if (mate && fin(mate.n) && mate.n > 0) {
    const head =
      mate.kind === "lost"
        ? `You had mate in ${mate.n}; after this move there was no forced mate.`
        : `This move allowed mate in ${mate.n}.`;
    const tail =
      a === b ? `Your win chance stayed at ${b}%.` : `Your win chance went from ${b}% to ${a}%.`;
    return `${head} ${tail}`;
  }
  if (a === b) return `Your win chance stayed at ${b}%.`;
  return a < b
    ? `Your win chance went from ${b}% to ${a}%, a drop of ${b - a} win% points.`
    : `Your win chance went from ${b}% to ${a}%, a rise of ${a - b} win% points.`;
}

/** The peer benchmark's blunders a game, for the overview headline (ADR 0080). */
export type HeadlinePeer = {
  platform: "chesscom" | "lichess";
  low: number;
  high: number;
  /** Distinct opponents behind the median. */
  players: number;
  /** The median of the opponents' blunders a game, or null when that row is not served. */
  blunders: number | null;
};

/**
 * The headline's comparison: the players faced, against the player's blunder range (Q1). With a
 * peer benchmark it is never "not compared with players at your level".
 */
function peerTail(bl: Estimate, peer: HeadlinePeer | null | undefined): string {
  if (!peer || !fin(peer.low) || !fin(peer.high)) return NOT_COMPARED;
  if (bl.kind !== "estimate" || !fin(bl.value) || peer.blunders === null || !fin(peer.blunders)) {
    return "Not compared with the players you've faced yet: too few of their games have a blunder count.";
  }
  const site = peer.platform === "chesscom" ? "chess.com" : "lichess";
  const n = Math.max(0, Math.trunc(peer.players));
  const m = peer.blunders;
  const iv = bl.interval && fin(bl.interval.low) && fin(bl.interval.high) ? bl.interval : null;
  const where = iv
    ? m > iv.high
      ? ", more than your likely range"
      : m < iv.low
        ? ", fewer than your likely range"
        : ", inside your likely range"
    : "";
  return `The players you've faced on ${site}, rated about ${count(peer.low)}–${count(peer.high)}, make about ${one(m)} (the median of ${count(n)} ${n === 1 ? "player" : "players"})${where}. ${BENCHMARK_BIAS_TEXT}`;
}

/** The overview headline in a game's terms: how often a winning chance is given away. */
export function headlineMeaning(h: {
  accuracy: Estimate;
  blunders: Estimate;
  /** "rapid games", "analysed games". */
  games: string;
  /** The players faced, when the scope has a peer benchmark. */
  peer?: HeadlinePeer | null;
}): string {
  const { accuracy: acc, blunders: bl } = h;
  const NOT_COMPARED = peerTail(bl, h.peer);
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

/** The peer group in running text, or null when the benchmark is not a peer band (ADR 0080). */
function peerGroup(b: Benchmark | null | undefined): string | null {
  if (b?.source !== "peers" || !fin(b.low) || !fin(b.high)) return null;
  const site = b.platform === "chesscom" ? "chess.com" : "lichess";
  // The count is named here, so the comparison is one sentence and no line repeats it (critic 8).
  const n = fin(b.players) ? Math.max(0, Math.trunc(b.players)) : null;
  const who = n === null ? "players" : `the ${count(n)} ${n === 1 ? "player" : "players"}`;
  return `${who} you've faced on ${site}, rated about ${count(b.low)}–${count(b.high)}`;
}

/** The states the weaknesses page files under "Costing you games" when the comparison backs them. */
export const COSTING_STATES: readonly WeaknessState[] = ["active", "emerging", "relapsed"];

/**
 * Whether a row's peer comparison backs its costing state (ADR 0084): the stored
 * `P(rate > benchmark)` is at least 0.7, the same threshold the state and the coach's focus use.
 */
export const comparisonBacksState = (state: WeaknessState, confidence: number) =>
  !COSTING_STATES.includes(state) || probablyAbove(confidence);

/**
 * You against the benchmark's median: a median outside the 80% range says more or less; inside
 * it, a stored confidence of 0.7 or more says "probably more often", with the odds (ADR 0084). A
 * costing state the comparison does not back says where it comes from, so the page never reads
 * as contradicting itself. A peer benchmark (players the user has faced) is named as such, never
 * as "players at your level".
 */
export function levelText(l: {
  rate: { mean: number; low: number; high: number };
  median: number | null;
  compared: boolean;
  sufficient: boolean;
  benchmark?: Benchmark | null;
  confidence?: number;
  state?: WeaknessState;
}): string {
  if (!l.compared || !fin(l.median)) return `${NO_BENCHMARK_TEXT}.`;
  const m = `${(l.median * 100).toFixed(1)}%`;
  const group = peerGroup(l.benchmark) ?? "typical players at your level";
  const Group = group.charAt(0).toUpperCase() + group.slice(1);
  if (!l.sufficient) return `${Group}: ${m}. A comparison waits for more chances.`;
  if (l.median < l.rate.low) {
    return `More often than ${group} (${m}): their rate is below your whole likely range.`;
  }
  if (l.median > l.rate.high) {
    return `Less often than ${group} (${m}): their rate is above your whole likely range.`;
  }
  if (probablyAbove(l.confidence)) {
    return `Probably more often than ${group} (${m}): about ${likelyText(l.confidence)}, though their rate is inside your likely range.`;
  }
  const plain = `Not clearly different from ${group} (${m}): their rate is inside your likely range.`;
  if (l.state && fin(l.confidence) && !comparisonBacksState(l.state, l.confidence)) {
    return `${plain} ${stateOriginText(l.state)}`;
  }
  return plain;
}

/** Where a costing state the comparison does not back comes from (ADR 0084). */
export function stateOriginText(state: WeaknessState): string {
  return `${stateStyle[state].label} comes from your game-by-game record (every game since the last change it found), not from this comparison.`;
}

/** "Your estimated rate is 4.2% of chances, likely between 3.1% and 5.7% (80% range)." */
export function rateText(r: { mean: number; low: number; high: number }, early: boolean): string {
  if (!fin(r.mean) || !fin(r.low) || !fin(r.high)) return "Your rate is not estimated yet.";
  const p = (v: number) => `${(v * 100).toFixed(1)}%`;
  return `Your ${early ? "early estimate" : "estimated rate"} is ${p(r.mean)} of chances, likely between ${p(r.low)} and ${p(r.high)} (80% range).`;
}

/** A stored opening line's verdict evidence (`opening_lines.summary`, Rust `LineSummary`). */
export type LineEvidence = {
  verdict: "keep" | "fix" | "learn" | "replace";
  color: "white" | "black";
  kind: "chosen" | "faced";
  games: number;
  /** Mean score minus expected score, -1..1 (0.01 is one percentage point). */
  perfVsExpected: number | null;
  perfGames: number;
  perfInterval: { low: number; high: number } | null;
  /** The most repeated move that lost 10+ win% points. */
  errorSpot: { san: string; games: number } | null;
  analysedGames: number;
  avgWinAtOpeningEnd: number | null;
  medianDeviationPly: number | null;
  /** `games` over the player's games with this colour, 0..1. */
  share: number;
  struggles: string[];
};

/** The key a report's `OpeningVerdict` and a stored line share. */
export const lineEvidenceKey = (lineKey: string, color: string, kind: string) =>
  `${lineKey}|${color}|${kind}`;

const MINUS = "−";
/** Percentage points with a sign: "−4", "+7", "0". */
const ppOf = (v: number) => {
  const p = Math.round(v * 100);
  return p === 0 ? "0" : p < 0 ? `${MINUS}${-p}` : `+${p}`;
};
/** Whole pp rounded outward, guarding float noise (0.13 × 100 is 13.000000000000002). */
const outward = (v: number, f: (x: number) => number) => f(Math.round(v * 1e6) / 1e4) / 100;
const rangeOf = (iv: LineEvidence["perfInterval"]) =>
  iv && fin(iv.low) && fin(iv.high)
    ? `likely ${ppOf(outward(iv.low, Math.floor))} to ${ppOf(outward(iv.high, Math.ceil))} pp`
    : null;
const rated = (n: number) => `${count(n)} rated ${n === 1 ? "game" : "games"}`;
const VS = "against what your rating predicts";
const KEEP = "Keep means no single move keeps costing you";

/**
 * Why a line has its verdict, in the order `decide_verdict` (profile-core
 * `openings/verdicts.rs`) applies its rules, from the stored evidence only.
 */
export function verdictWhy(e: LineEvidence): string {
  const perf = fin(e.perfVsExpected) ? e.perfVsExpected : null;
  const range = rangeOf(e.perfInterval);
  switch (e.verdict) {
    case "fix":
      return e.errorSpot && fin(e.errorSpot.games)
        ? `Fix means one move keeps costing you: ${e.errorSpot.san}, in ${count(e.errorSpot.games)} of your ${count(e.games)} games in this line, losing ${V.deviationDeltaWinPct} win% points or more each time.`
        : `Fix means one move keeps costing you in this line, losing ${V.deviationDeltaWinPct} win% points or more each time it is played.`;
    case "replace": {
      const head = "Replace means this line keeps scoring well below what your rating predicts";
      const rule = `Replace needs at least ${V.replaceMinGames} games, ${MINUS}${V.replaceMaxPerfPp} pp or worse, and the whole range below zero.`;
      if (perf === null) return `${head}. ${rule}`;
      const r = range ? ` (${range}, all below zero)` : "";
      return `${head}: ${ppOf(perf)} pp over ${rated(e.perfGames)}${r}. ${rule}`;
    }
    case "learn": {
      const ply = e.medianDeviationPly;
      if (
        e.kind === "chosen" &&
        fin(ply) &&
        ply <= V.learnMaxDeviationPly &&
        fin(e.share) &&
        e.share * 100 >= V.learnMinSharePct - 1e-9
      ) {
        const colour = e.color === "white" ? "White" : "Black";
        return `Learn deeper means you play it often (${Math.round(e.share * 100)}% of your games as ${colour}) but usually leave the known moves by move ${Math.ceil(Math.round(ply) / 2)}.`;
      }
      if (e.struggles.includes("worse_after_opening") && fin(e.avgWinAtOpeningEnd)) {
        return `Learn deeper means you leave the opening worse off: your win chance averages ${Math.round(e.avgWinAtOpeningEnd)}% when it ends, over ${count(e.analysedGames)} analysed games.`;
      }
      if (e.struggles.includes("score_below_expected") && perf !== null) {
        const r = range ? ` (${range} over ${rated(e.perfGames)})` : ` over ${rated(e.perfGames)}`;
        return `Learn deeper means you score below what your rating predicts here: ${ppOf(perf)} pp${r}, short of the ${V.replaceMinGames} games and ${MINUS}${V.replaceMaxPerfPp} pp that would mean replacing it.`;
      }
      return "Learn deeper means this line shows a struggle that more study of it should ease.";
    }
    default: {
      if (perf === null) {
        return `${KEEP} over your ${count(e.games)} games here, and there is no comparison with your rating yet.`;
      }
      const iv = e.perfInterval;
      if (!range || !iv) {
        return `${KEEP}, and your ${ppOf(perf)} pp ${VS} has no range yet (${rated(e.perfGames)}).`;
      }
      if (iv.low > 0) {
        return `${KEEP}, and you score above what your rating predicts: ${ppOf(perf)} pp over ${rated(e.perfGames)} (${range}).`;
      }
      if (iv.high < 0) {
        return `${KEEP}, and your ${ppOf(perf)} pp ${VS} (${range} over ${rated(e.perfGames)}) is short of the ${MINUS}${V.perfDropPp} pp that counts as a struggle.`;
      }
      return `${KEEP}, and your ${ppOf(perf)} pp ${VS} is within normal variation for ${rated(e.perfGames)} (${range}).`;
    }
  }
}
