import type { Db } from "@ci/db";
import {
  bandReference,
  gameMetrics,
  gameOpenings,
  games,
  linkedAccounts,
  openingLines,
  referenceBuilds,
  reportRuns,
  weaknessEstimates,
  weaknessStateHistory,
} from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, asc, count, desc, eq, gte, inArray, isNotNull, lt, type SQL, sql } from "drizzle-orm";
import type { DayRange } from "../filters/params";
import { readOnly } from "../games";
import type { TimeClass } from "../games/types";
import { BOOTSTRAP_RESAMPLES, CONFIDENCE, fnv1a, mulberry32, regIncBeta, wilson } from "../stats";
import { type Benchmark, getBenchmark } from "./benchmark";
import { type KpiDef, kpiDef, seriesLabel, weaknessDef } from "./catalogue";
import { difficultyStatus } from "./difficulty";
import { KPI_GLOSSARY } from "./glossary";
import { CURRENT_WINDOW, latestSnapshots } from "./kpis";
import { ownsScope } from "./scope";
import type { Area, CoachScope, Side as Colour, DifficultyStatus, WeaknessState } from "./types";
import { openingNames } from "./weaknesses";

// The strengths page's read model (`/app/strengths`): the good things in a scope's games and what
// to keep doing. Every claim passes one gate below, and each gate refuses to claim without its
// minimum evidence; the page says "not enough games yet" instead. The gates are pure (unit tested
// one by one); `loadStrengths` only reads rows and hands them over.

// ---- Constants (mirrors of the Rust rules they cite) ----

/** "Your last 50 games": the recent side of the own-baseline comparison (kpi `last50`). */
export const RECENT_GAMES = 50;
/** Per-game rows read at most, newest first. */
export const MAX_GAMES = 500;
/** profile-core `timeline/series.rs` `CONFIRM_P_CHANGE`. */
export const CONFIRM_P_CHANGE = 0.8;
/** profile-core `timeline/series.rs` `CONFIRM_REL_CHANGE`. */
export const CONFIRM_REL_CHANGE = 0.15;
/** An opening line needs this many games with an expected score before it is judged. */
export const MIN_OPENING_GAMES = 10;
/** Maia: a best move that at most this share of players at the level finds is a hard one. */
export const HARD_P_BEST = 0.3;
/** profile-core `features/derive.rs` `CRITICAL_GAP`: best line ahead of the second by this W%. */
export const CRITICAL_GAP = 15;
/**
 * The fallback's balanced band (N11): the mover's win% before the move. Outside it the game was
 * already decided, and the "only move" was usually just taking what was there.
 */
export const BALANCED_MIN = 25;
export const BALANCED_MAX = 75;
/** A "promising" opening is at least this likely to score above its expected score (N13). */
export const PROMISING_P = 0.8;
/** Boards in "Your best moves in hard spots". */
export const MAX_SPOTS = 3;
/** refdata `band_reference.rs` `MAX_FALLBACK_BANDS`. */
export const MAX_FALLBACK_BANDS = 2;

export const BANDS = ["B0", "B1", "B2", "B3", "B4", "B5"] as const;
export type Band = (typeof BANDS)[number];

/** profile-core `constants.rs` `chesscom_to_lichess_offset`. */
const CHESSCOM_OFFSET: Record<TimeClass, number> = {
  bullet: 250,
  blitz: 250,
  rapid: 350,
  classical: 300,
  daily: 300,
};

/** The metrics the page compares, in page order (each in the KPI catalogue). */
export const STRENGTH_METRICS = [
  "accuracy",
  "accuracy.opening",
  "accuracy.middlegame",
  "accuracy.endgame",
  "blunders_per_game",
  "conversion_rate",
  "resilience_rate",
] as const;

const defs = (): KpiDef[] =>
  STRENGTH_METRICS.map(kpiDef).filter((d): d is KpiDef => d != null && d.higherIsBetter != null);

const fin = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

// ---- 1. Where you're above your level ----

/** profile-core `bands.rs` `band_of(to_lichess_equivalent(..))`; null without a rating. */
export function bandOf(
  platform: "lichess" | "chesscom",
  tc: TimeClass,
  rating: number | null,
): Band | null {
  if (!fin(rating)) return null;
  const r = platform === "chesscom" ? rating + CHESSCOM_OFFSET[tc] : rating;
  if (r < 900) return "B0";
  if (r < 1300) return "B1";
  if (r < 1700) return "B2";
  if (r < 2000) return "B3";
  if (r < 2300) return "B4";
  return "B5";
}

export type BandMedian = { median: number; band: Band };

/**
 * Per metric, the median of the nearest band within `MAX_FALLBACK_BANDS`, the lower band on a tie
 * (Rust `load_band_reference`). The rows are already the active build's non-low-sample ones.
 */
export function bandMedians(
  rows: { band: string; metric: string; median: number }[],
  band: Band,
): Map<string, BandMedian> {
  const at = BANDS.indexOf(band);
  const best = new Map<string, { key: [number, number]; v: BandMedian }>();
  for (const r of rows) {
    const pos = BANDS.indexOf(r.band as Band);
    if (pos < 0 || !fin(r.median)) continue;
    const dist = Math.abs(pos - at);
    if (dist > MAX_FALLBACK_BANDS) continue;
    const have = best.get(r.metric);
    if (have && (have.key[0] < dist || (have.key[0] === dist && have.key[1] <= pos))) continue;
    best.set(r.metric, { key: [dist, pos], v: { median: r.median, band: r.band as Band } });
  }
  return new Map([...best].map(([m, b]) => [m, b.v]));
}

export type BandClaim = {
  metric: string;
  label: string;
  unit: KpiDef["unit"];
  higherIsBetter: boolean;
  value: number;
  low: number;
  high: number;
  n: number;
  median: number;
  band: Band;
};

/**
 * The KPIs whose stored last-50 interval lies wholly on the better side of the band median, with
 * at least the catalogue's minimum games. An interval that touches the median is no claim.
 */
export function aboveBand(
  current: {
    metric: string;
    value: number | null;
    low: number | null;
    high: number | null;
    n: number;
  }[],
  medians: Map<string, BandMedian>,
): BandClaim[] {
  const out: BandClaim[] = [];
  for (const def of defs()) {
    const row = current.find((r) => r.metric === def.metric);
    const ref = medians.get(def.metric);
    if (!row || !ref || row.n < def.minN) continue;
    const { value, low, high } = row;
    if (!fin(value) || !fin(low) || !fin(high) || low > high) continue;
    const above = def.higherIsBetter ? low > ref.median : high < ref.median;
    if (!above) continue;
    out.push({
      metric: def.metric,
      label: def.label,
      unit: def.unit,
      higherIsBetter: def.higherIsBetter === true,
      value,
      low,
      high,
      n: row.n,
      median: ref.median,
      band: ref.band,
    });
  }
  return out;
}

/** One served `account_benchmark_stat` row of the peer benchmark (ADRs 0080, 0081). */
export type PeerRow = {
  metric: string;
  median: number;
  /** The pooled rate of a rate row; null for a mean row (the median of the opponents' means). */
  pooledRate: number | null;
  /** A rate row's raw chances in the window; null for a mean row. */
  nChances: number | null;
  nPlayers: number;
};

export type PeerClaim = Omit<BandClaim, "median" | "band"> & {
  /** What the player is compared with: the pooled rate, or the median of the opponents' means. */
  peer: number;
  pooled: boolean;
  chances: number | null;
  players: number;
};

/**
 * The KPIs whose stored last-50 interval lies wholly on the better side of the peers' compared
 * value (the pooled rate where the row has one, else its median), with the catalogue's minimum
 * games. A measure the peer benchmark does not compare (ADR 0081 mirrors) has no row: no claim.
 */
export function abovePeers(
  current: {
    metric: string;
    value: number | null;
    low: number | null;
    high: number | null;
    n: number;
  }[],
  peers: PeerRow[],
): PeerClaim[] {
  const out: PeerClaim[] = [];
  for (const def of defs()) {
    const row = current.find((r) => r.metric === def.metric);
    const ref = peers.find((p) => p.metric === def.metric);
    if (!row || !ref || row.n < def.minN) continue;
    const pooled = fin(ref.pooledRate);
    const peer = pooled ? (ref.pooledRate as number) : ref.median;
    const { value, low, high } = row;
    if (!fin(peer) || !fin(value) || !fin(low) || !fin(high) || low > high) continue;
    if (!(def.higherIsBetter ? low > peer : high < peer)) continue;
    out.push({
      metric: def.metric,
      label: def.label,
      unit: def.unit,
      higherIsBetter: def.higherIsBetter === true,
      value,
      low,
      high,
      n: row.n,
      peer,
      pooled,
      chances: fin(ref.nChances) ? ref.nChances : null,
      players: ref.nPlayers,
    });
  }
  return out;
}

/** The page's per-game values of one `game_metrics.metrics` row; a missing or bad field is absent. */
export function gameValues(metrics: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  if (!isObj(metrics)) return out;
  if (fin(metrics.accuracy)) out.accuracy = metrics.accuracy;
  const phase = isObj(metrics.accuracyByPhase) ? metrics.accuracyByPhase : {};
  for (const p of ["opening", "middlegame", "endgame"] as const) {
    if (fin(phase[p])) out[`accuracy.${p}`] = phase[p] as number;
  }
  if (fin(metrics.blunders) && metrics.blunders >= 0) out.blunders_per_game = metrics.blunders;
  const conv = isObj(metrics.conversion) ? metrics.conversion : {};
  if (conv.episode === true && typeof conv.converted === "boolean") {
    out.conversion_rate = conv.converted ? 1 : 0;
  }
  const res = isObj(metrics.resilience) ? metrics.resilience : {};
  if (res.episode === true && typeof res.saved === "boolean") {
    out.resilience_rate = res.saved ? 1 : 0;
  }
  return out;
}

export type Side = { value: number; n: number };
export type OwnClaim = {
  metric: string;
  label: string;
  unit: KpiDef["unit"];
  higherIsBetter: boolean;
  recent: Side;
  earlier: Side;
  /** 80% percentile-bootstrap interval of recent minus earlier. */
  diff: { low: number; high: number };
};

export type OwnTrends = {
  claims: OwnClaim[];
  /** Whether at least one metric had its minimum on both sides. */
  enough: boolean;
  /** Games with at least one of the page's measures. */
  measured: number;
  /** Games still needed before the first metric can be compared (0 when enough). */
  needGames: number;
};

const mean = (v: number[]) => v.reduce((a, b) => a + b, 0) / v.length;

/** Percentile bootstrap of mean(a) - mean(b), seeded; null when it has no spread. */
function bootstrapDiff(
  a: number[],
  b: number[],
  seed: string,
): { low: number; high: number } | null {
  const rnd = mulberry32(fnv1a(seed));
  const draws = new Float64Array(BOOTSTRAP_RESAMPLES);
  const resample = (v: number[]) => {
    let s = 0;
    for (let i = 0; i < v.length; i++) s += v[Math.floor(rnd() * v.length)] as number;
    return s / v.length;
  };
  for (let r = 0; r < BOOTSTRAP_RESAMPLES; r++) draws[r] = resample(a) - resample(b);
  draws.sort();
  const tail = (1 - CONFIDENCE) / 2;
  const at = (q: number) =>
    draws[
      Math.min(BOOTSTRAP_RESAMPLES - 1, Math.max(0, Math.floor(q * BOOTSTRAP_RESAMPLES)))
    ] as number;
  const low = at(tail);
  const high = at(1 - tail);
  return fin(low) && fin(high) && high > low ? { low, high } : null;
}

/**
 * Own baseline: the newest `RECENT_GAMES` games against every earlier one, per metric. A metric
 * needs its catalogue minimum on both sides, and is claimed only when the 80% bootstrap interval
 * of the difference lies wholly on the better side of zero. `games` is oldest first.
 */
export function ownTrends(games: Record<string, number>[], seed: string): OwnTrends {
  const recent = games.slice(-RECENT_GAMES);
  const earlier = games.slice(0, Math.max(0, games.length - RECENT_GAMES));
  const claims: OwnClaim[] = [];
  let enough = false;
  for (const def of defs()) {
    const r = recent.map((g) => g[def.metric]).filter(fin);
    const e = earlier.map((g) => g[def.metric]).filter(fin);
    if (r.length < def.minN || e.length < def.minN) continue;
    enough = true;
    const diff = bootstrapDiff(r, e, `${seed}:${def.metric}`);
    if (!diff) continue;
    const better = def.higherIsBetter ? diff.low > 0 : diff.high < 0;
    if (!better) continue;
    claims.push({
      metric: def.metric,
      label: def.label,
      unit: def.unit,
      higherIsBetter: def.higherIsBetter === true,
      recent: { value: mean(r), n: r.length },
      earlier: { value: mean(e), n: e.length },
      diff,
    });
  }
  // The first comparable metric needs its minimum on top of the recent games.
  const present = defs().filter((d) => games.some((g) => fin(g[d.metric])));
  const minN = Math.min(...(present.length > 0 ? present : defs()).map((d) => d.minN));
  const measured = games.filter((g) => defs().some((d) => fin(g[d.metric]))).length;
  return {
    claims,
    enough,
    measured,
    needGames: enough ? 0 : Math.max(1, RECENT_GAMES + minN - measured),
  };
}

// ---- 2. What's improving ----

export type WeaknessChangeInput = {
  seriesKey: string;
  label: string;
  state: WeaknessState;
  /** The estimate has its minimum evidence. */
  sufficient: boolean;
  chances: number;
  /** The newest transition into the current state. */
  transition: {
    toState: WeaknessState;
    pChange: number | null;
    rateBefore: number | null;
    rateAfter: number | null;
    at: Date | null;
    counts: { events: number; chances: number } | null;
  } | null;
};

export type ImprovingWeakness = {
  seriesKey: string;
  label: string;
  state: "improving" | "resolved";
  before: number;
  after: number;
  pChange: number;
  at: Date | null;
  afterCounts: { events: number; chances: number } | null;
  chances: number;
};

/**
 * Weaknesses now improving or resolved whose change into that state was confirmed by the
 * timeline's own rule: `p_change ≥ CONFIRM_P_CHANGE` and a relative drop of at least
 * `CONFIRM_REL_CHANGE`, on an estimate with its minimum evidence.
 */
export function improvingWeaknesses(rows: WeaknessChangeInput[]): ImprovingWeakness[] {
  const out: ImprovingWeakness[] = [];
  for (const w of rows) {
    if (w.state !== "improving" && w.state !== "resolved") continue;
    const t = w.transition;
    if (!w.sufficient || !t || t.toState !== w.state) continue;
    const { pChange, rateBefore: b, rateAfter: a } = t;
    if (!fin(pChange) || !fin(b) || !fin(a) || b <= 0 || a < 0) continue;
    if (pChange < CONFIRM_P_CHANGE || (b - a) / b < CONFIRM_REL_CHANGE) continue;
    out.push({
      seriesKey: w.seriesKey,
      label: w.label,
      state: w.state,
      before: b,
      after: a,
      pChange,
      at: t.at,
      afterCounts: t.counts,
      chances: w.chances,
    });
  }
  return out;
}

// ---- 3. Openings that work for you ----

export type Verdict = "keep" | "fix" | "learn" | "replace";

export type OpeningInput = {
  lineKey: string;
  name: string;
  color: "white" | "black";
  kind: "chosen" | "faced";
  verdict: Verdict | null;
  games: { score: number; expected: number }[];
};

export type WorkingOpening = Omit<OpeningInput, "games"> & {
  n: number;
  score: number;
  expected: number;
  /** 80% Wilson interval of the score. */
  low: number;
  high: number;
};

/**
 * Lines with at least `MIN_OPENING_GAMES` games (each with a score and a rating-predicted
 * expected score) whose 80% Wilson interval of the score lies wholly above the mean expected
 * score. Best margin first.
 */
export function workingOpenings(lines: OpeningInput[]): WorkingOpening[] {
  const out: WorkingOpening[] = [];
  for (const l of lines) {
    const g = l.games.filter((x) => fin(x.score) && fin(x.expected));
    if (g.length < MIN_OPENING_GAMES) continue;
    const points = g.reduce((s, x) => s + x.score, 0);
    const expected = mean(g.map((x) => x.expected));
    const w = wilson(points, g.length);
    if (!w || w.low <= expected) continue;
    const { games: _games, ...rest } = l;
    out.push({
      ...rest,
      n: g.length,
      score: points / g.length,
      expected,
      low: w.low,
      high: w.high,
    });
  }
  return out.sort((a, b) => b.low - b.expected - (a.low - a.expected) || b.n - a.n);
}

export type PromisingOpening = Omit<WorkingOpening, "low" | "high"> & {
  /** Posterior probability that the line's true score is above the expected score. */
  pAbove: number;
};

/**
 * The "promising" tier (N13): lines with at least `MIN_OPENING_GAMES` games, not clearly above
 * (`workingOpenings`), whose score is at least `PROMISING_P` likely above the mean expected
 * score under a Beta(1 + points, 1 + n - points) posterior (a uniform prior; a draw counts
 * half). Likeliest first.
 */
export function promisingOpenings(lines: OpeningInput[]): PromisingOpening[] {
  const clear = new Set(workingOpenings(lines).map((o) => `${o.color}:${o.kind}:${o.lineKey}`));
  const out: PromisingOpening[] = [];
  for (const l of lines) {
    if (clear.has(`${l.color}:${l.kind}:${l.lineKey}`)) continue;
    const g = l.games.filter((x) => fin(x.score) && fin(x.expected));
    if (g.length < MIN_OPENING_GAMES) continue;
    const points = Math.min(
      g.length,
      Math.max(
        0,
        g.reduce((s, x) => s + x.score, 0),
      ),
    );
    const expected = mean(g.map((x) => x.expected));
    if (!(expected > 0 && expected < 1)) continue;
    const pAbove = 1 - regIncBeta(expected, 1 + points, 1 + g.length - points);
    if (!fin(pAbove) || pAbove < PROMISING_P) continue;
    const { games: _games, ...rest } = l;
    out.push({ ...rest, n: g.length, score: points / g.length, expected, pAbove });
  }
  return out.sort((a, b) => b.pAbove - a.pAbove || b.n - a.n);
}

// ---- 4. Your best moves in hard spots ----

export type StrengthKind = "only_move_found" | "tactic_found";

export type HardSpotCandidate = {
  gameId: string;
  playedAt: Date;
  ply: number;
  kind: StrengthKind;
  /** The position before the move. */
  fen: string;
  san: string;
  uci: string;
  bestUci: string | null;
  /** Mover's win%, 0 to 100. */
  winBefore: number;
  winAfter: number;
  timeSpent: number | null;
  difficultyStatus: string | null;
  pBest: number | null;
  isCritical: boolean | null;
  multipvGap: number | null;
  /** The opponent's move just before (its SAN and stored judgement), when there is one. */
  prevSan: string | null;
  prevJudgement: string | null;
};

export type HardSpots = {
  /** `maia`: Maia rated them hard; `critical`: the stated fallback; `none`: nothing qualifies. */
  mode: "maia" | "critical" | "none";
  spots: HardSpotCandidate[];
};

/** The engine's move, when the row records one (the strength ply already says it was played). */
const playedBest = (c: HardSpotCandidate) => c.bestUci == null || c.bestUci === c.uci;

const VALUE: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9, k: 100 };

/**
 * True for a capture anyone would find (N11): a recapture on the square the opponent just
 * captured on, a capture of more than the capturing piece is worth, or one nothing can legally
 * take back. False for a quiet move or a move that is not legal in `fen`.
 */
export function obviousCapture(fen: string, uci: string, prevSan: string | null): boolean {
  let chess: Chess;
  let move: ReturnType<Chess["move"]>;
  try {
    chess = new Chess(fen);
    move = chess.move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      ...(uci.length > 4 ? { promotion: uci.slice(4, 5) } : {}),
    });
  } catch {
    return false;
  }
  if (!move.captured) return false;
  if (prevSan?.includes("x") && prevSan.replace(/[+#?!]/g, "").endsWith(move.to)) return true;
  if ((VALUE[move.captured] ?? 0) > (VALUE[move.piece] ?? 0)) return true;
  return !chess.moves({ verbose: true }).some((m) => m.to === move.to && m.captured);
}

/**
 * A move anyone would find: an obvious capture (`obviousCapture`). Finding the refutation of
 * the opponent's mistake is kept: punishing errors is a strength, and on the owner's real games
 * excluding every post-mistake moment left 2 of 69 balanced critical moments, both free
 * captures. The balanced-position gate still drops moments their error already decided.
 */
const handedOver = (c: HardSpotCandidate) => obviousCapture(c.fen, c.uci, c.prevSan);

const balanced = (c: HardSpotCandidate) =>
  c.winBefore >= BALANCED_MIN && c.winBefore <= BALANCED_MAX;

/** One per game first, then the rest, keeping `sorted`'s order. */
function spread(sorted: HardSpotCandidate[]): HardSpotCandidate[] {
  const seen = new Set<string>();
  const first: HardSpotCandidate[] = [];
  const rest: HardSpotCandidate[] = [];
  for (const c of sorted) {
    (seen.has(c.gameId) ? rest : first).push(c);
    seen.add(c.gameId);
  }
  return [...first, ...rest].slice(0, MAX_SPOTS);
}

/**
 * Up to `MAX_SPOTS` positions where the player found the engine's best move, never an obvious
 * capture (`obviousCapture`; refuting the opponent's mistake with a real move counts). Maia-hard
 * ones (difficulty `ok`, `p_best ≤ HARD_P_BEST`) first, hardest first; with none, the fallback is
 * critical moments in a roughly balanced game (`is_critical`, the best line at least
 * `CRITICAL_GAP` win% ahead of the second, the win% before between `BALANCED_MIN` and
 * `BALANCED_MAX`), biggest gap first, newest on a tie.
 */
export function pickHardSpots(candidates: HardSpotCandidate[]): HardSpots {
  const ok = candidates.filter((c) => playedBest(c) && !handedOver(c));
  const maia = ok.filter(
    (c) => c.difficultyStatus === "ok" && fin(c.pBest) && c.pBest <= HARD_P_BEST,
  );
  if (maia.length > 0) {
    const sorted = [...maia].sort(
      (a, b) =>
        (a.pBest as number) - (b.pBest as number) || b.playedAt.getTime() - a.playedAt.getTime(),
    );
    return { mode: "maia", spots: spread(sorted) };
  }
  const critical = ok.filter(
    (c) =>
      c.isCritical === true && fin(c.multipvGap) && c.multipvGap >= CRITICAL_GAP && balanced(c),
  );
  if (critical.length === 0) return { mode: "none", spots: [] };
  const sorted = [...critical].sort(
    (a, b) =>
      (b.multipvGap as number) - (a.multipvGap as number) ||
      b.playedAt.getTime() - a.playedAt.getTime() ||
      a.ply - b.ply,
  );
  return { mode: "critical", spots: spread(sorted) };
}

/** The glossary entry whose tip a strength kind keeps doing. */
const KEEP_DOING_KEY: Record<StrengthKind, string> = {
  only_move_found: "sharp_error_rate",
  tactic_found: "tactics_missed_per_game",
};

/** A find in fewer seconds than this was quick: "take your time" would contradict it (F3). */
export const FAST_FIND_SECONDS = 5;

/** The note of a quick find, by kind. */
const FAST_NOTE: Record<StrengthKind, string> = {
  only_move_found:
    "You saw the only move quickly. Keep scanning checks, captures and threats first.",
  tactic_found: "You spotted the tactic quickly. Keep scanning checks, captures and threats first.",
};

/**
 * The note under a found move, chosen by its kind and the seconds it took (holistic F3): a quick
 * find says so; one that took its time gets the kind's glossary tip; with no clock, the tactics
 * tip, which never says "take your time". Null when the glossary entry is missing.
 */
export function keepDoingNote(kind: StrengthKind, timeSpent: number | null): string | null {
  if (timeSpent !== null && fin(timeSpent) && timeSpent >= 0 && timeSpent < FAST_FIND_SECONDS) {
    return FAST_NOTE[kind];
  }
  const key =
    timeSpent === null || !fin(timeSpent) ? KEEP_DOING_KEY.tactic_found : KEEP_DOING_KEY[kind];
  const tip = KPI_GLOSSARY[key]?.tip;
  if (!tip) return null;
  return `Keep doing this: ${tip.charAt(0).toLowerCase()}${tip.slice(1)}`;
}

/** A hard spot's key in `notes`: its game and ply. */
export const spotKey = (s: { gameId: string; ply: number }) => `${s.gameId}:${s.ply}`;

// ---- 5. The report's strengths ----

const AREA_IDS: readonly Area[] = [
  "openings",
  "middlegame",
  "endgame",
  "tactics",
  "calculation",
  "time",
  "mindset",
];

export type ReportStrength = {
  area: Area;
  label: string;
  metricKey: string | null;
  examples: number;
};

/**
 * The strengths of a `report_runs.input` (Rust `select_strengths`: areas scoring at least 60,
 * weaknesses with `1 - confidence ≥ 0.8`, each with an exemplar), so the page, the report and the
 * coach name the same ones. Malformed entries are dropped.
 */
export function reportStrengths(input: unknown): ReportStrength[] {
  if (!isObj(input) || !Array.isArray(input.strengths)) return [];
  return input.strengths.flatMap((s): ReportStrength[] => {
    if (!isObj(s) || typeof s.label !== "string" || s.label.trim() === "") return [];
    if (!AREA_IDS.includes(s.area as Area)) return [];
    return [
      {
        area: s.area as Area,
        label: s.label,
        metricKey: typeof s.metricKey === "string" ? s.metricKey : null,
        examples: Array.isArray(s.exemplars) ? s.exemplars.length : 0,
      },
    ];
  });
}

// ---- The page model ----

/** One side's comparison with other players: the peers' rows, the band's, or none stored. */
export type SideAbove =
  | { mode: "peers"; claims: PeerClaim[]; compared: number }
  | { mode: "band"; band: Band; claims: BandClaim[]; compared: number }
  | { mode: "none" };

export type StrengthsModel = {
  /** The colour filter's side (ADR 0082) and the date filter the page was read with. */
  side: Colour;
  range: DayRange;
  /** Analysed games of the scope's side and dates. */
  analysedGames: number;
  above:
    | {
        mode: "peers";
        benchmark: Extract<Benchmark, { source: "peers" }>;
        claims: PeerClaim[];
        compared: number;
      }
    | { mode: "band"; band: Band; claims: BandClaim[]; compared: number }
    | { mode: "own"; trends: OwnTrends };
  /** In Both mode, each colour's own comparison with other players; null for one colour. */
  bySide: { white: SideAbove; black: SideAbove } | null;
  improving: { weaknesses: ImprovingWeakness[]; kpis: OwnTrends | null };
  openings: { lines: WorkingOpening[]; promising: PromisingOpening[]; judged: number };
  /** One note per spot, keyed by [`spotKey`]. */
  hardSpots: HardSpots & { notes: Record<string, string | null> };
  difficulty: DifficultyStatus;
  report: { at: Date; strengths: ReportStrength[] } | null;
};

/** The start of the UTC day after `day`, so a range's `to` includes its whole day. */
const dayAfter = (day: string) => new Date(Date.parse(`${day}T00:00:00Z`) + 86_400_000);

/**
 * The scope's strengths, every read behind its `ownsScope` check; null for a foreign scope.
 * `scope.side` picks the colour (ADR 0082): the stored peer, band and KPI rows of that side, and
 * the games the player had that colour in. `range` narrows the per-game reads (the comparison
 * with earlier games, the openings and the hard spots); the stored last-50 rows and the
 * weakness timeline (every game's, both colours) have no dates.
 */
export async function loadStrengths(
  db: Db,
  userId: string,
  scope: CoachScope,
  range: DayRange = { from: null, to: null },
): Promise<StrengthsModel | null> {
  const side: Colour = scope.side ?? "both";
  // The peer benchmark (ADRs 0080, 0081), when the scope's last build-profile stored one.
  const benchmark = await getBenchmark(db, userId, scope);
  const model = await readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return null;
    // The games of the side and dates, through `game_metrics` (joined to `games` for the colour).
    const gameConds: SQL[] = [
      eq(gameMetrics.accountId, scope.accountId),
      eq(gameMetrics.timeClass, scope.timeClass),
    ];
    if (side !== "both") gameConds.push(eq(games.playerColor, side));
    if (range.from) gameConds.push(gte(gameMetrics.playedAt, new Date(`${range.from}T00:00:00Z`)));
    if (range.to) gameConds.push(lt(gameMetrics.playedAt, dayAfter(range.to)));
    const inGames = and(...gameConds);
    const filtered = side !== "both" || range.from !== null || range.to !== null;

    // Per-game values, newest first, then oldest first for the comparison.
    const rows = await tx
      .select({ gameId: gameMetrics.gameId, metrics: gameMetrics.metrics })
      .from(gameMetrics)
      .innerJoin(games, eq(games.id, gameMetrics.gameId))
      .where(inGames)
      .orderBy(desc(gameMetrics.playedAt), desc(gameMetrics.gameId))
      .limit(MAX_GAMES);
    const [selected] = filtered
      ? await tx
          .select({ n: count() })
          .from(gameMetrics)
          .innerJoin(games, eq(games.id, gameMetrics.gameId))
          .where(inGames)
      : [];
    const values = rows.reverse().map((r) => gameValues(r.metrics));
    const own = ownTrends(values, `${scope.accountId}:${scope.timeClass}:${side}`);

    // The band: the newest own rating on the Lichess scale (Rust `player_band`).
    const [rated] = await tx
      .select({
        rating: sql<
          number | null
        >`case when ${games.playerColor} = 'white' then ${games.whiteRating} else ${games.blackRating} end`,
        platform: linkedAccounts.platform,
      })
      .from(games)
      .innerJoin(linkedAccounts, eq(linkedAccounts.id, games.accountId))
      .where(
        and(
          eq(games.accountId, scope.accountId),
          eq(games.timeClass, scope.timeClass),
          sql`(case when ${games.playerColor} = 'white' then ${games.whiteRating} else ${games.blackRating} end) is not null`,
        ),
      )
      .orderBy(desc(games.playedAt), asc(games.id))
      .limit(1);
    const band = rated ? bandOf(rated.platform, scope.timeClass, rated.rating) : null;

    // One side's comparison with other players: its own peer rows (A1: never another side's),
    // its own band rows, against its own last-50 snapshots.
    const compareAt = async (at: Colour): Promise<SideAbove> => {
      const refRows = band
        ? await tx
            .select({
              band: bandReference.band,
              metric: bandReference.metric,
              median: bandReference.median,
            })
            .from(bandReference)
            .innerJoin(referenceBuilds, eq(referenceBuilds.id, bandReference.buildId))
            .where(
              and(
                eq(referenceBuilds.kind, "band_reference"),
                eq(referenceBuilds.status, "active"),
                eq(bandReference.timeClass, scope.timeClass),
                eq(bandReference.side, at),
                eq(bandReference.lowSample, false),
                inArray(bandReference.metric, [...STRENGTH_METRICS]),
              ),
            )
        : [];
      const peerRows: PeerRow[] =
        benchmark?.source === "peers"
          ? (
              (await tx.execute(sql`
                select metric, median, pooled_rate, n_chances, n_players
                from account_benchmark_stat
                where account_id = ${scope.accountId} and time_class = ${scope.timeClass}::time_class
                  and side = ${at}::side_scope and not low_sample and metric in (${sql.join(
                    STRENGTH_METRICS.map((m) => sql`${m}`),
                    sql`, `,
                  )})`)) as unknown as Record<string, unknown>[]
            ).flatMap((r): PeerRow[] => {
              const median = Number(r.median);
              if (!fin(median)) return [];
              const opt = (v: unknown) => (v == null || !fin(Number(v)) ? null : Number(v));
              return [
                {
                  metric: String(r.metric),
                  median,
                  pooledRate: opt(r.pooled_rate),
                  nChances: opt(r.n_chances),
                  nPlayers: Math.max(0, Math.trunc(Number(r.n_players) || 0)),
                },
              ];
            })
          : [];
      const current =
        peerRows.length > 0 || (band && refRows.length > 0)
          ? (
              await latestSnapshots(
                tx,
                { ...scope, side: at },
                [...STRENGTH_METRICS],
                CURRENT_WINDOW,
              )
            ).map((r) => ({
              metric: r.metric,
              value: r.value,
              low: r.ciLow,
              high: r.ciHigh,
              n: r.n,
            }))
          : [];
      const compared = defs().filter((d) => peerRows.some((p) => p.metric === d.metric)).length;
      if (benchmark?.source === "peers" && compared > 0) {
        return { mode: "peers", claims: abovePeers(current, peerRows), compared };
      }
      if (band && refRows.length > 0) {
        const medians = bandMedians(refRows, band);
        return { mode: "band", band, claims: aboveBand(current, medians), compared: medians.size };
      }
      return { mode: "none" };
    };
    const at = await compareAt(side);
    let above: StrengthsModel["above"] = { mode: "own", trends: own };
    if (at.mode === "peers" && benchmark?.source === "peers") above = { ...at, benchmark };
    else if (at.mode === "band") above = at;
    const bySide =
      side === "both" ? { white: await compareAt("white"), black: await compareAt("black") } : null;

    // Weaknesses improving or resolved, with their newest transition into that state.
    const estimates = await tx
      .select({
        seriesKey: weaknessEstimates.seriesKey,
        state: weaknessEstimates.state,
        nUnweighted: weaknessEstimates.nUnweighted,
      })
      .from(weaknessEstimates)
      .where(
        and(
          eq(weaknessEstimates.accountId, scope.accountId),
          eq(weaknessEstimates.timeClass, scope.timeClass),
          // The timeline is every game's (ADR 0082), so its states are the both estimates' (A1).
          eq(weaknessEstimates.side, "both"),
          inArray(weaknessEstimates.state, ["improving", "resolved"]),
        ),
      );
    const keys = estimates.map((e) => e.seriesKey);
    const transitions = keys.length
      ? await tx
          .select()
          .from(weaknessStateHistory)
          .where(
            and(
              eq(weaknessStateHistory.accountId, scope.accountId),
              eq(weaknessStateHistory.timeClass, scope.timeClass),
              inArray(weaknessStateHistory.seriesKey, keys),
              eq(weaknessStateHistory.kind, "transition"),
            ),
          )
          .orderBy(asc(weaknessStateHistory.id))
      : [];
    const names = await openingNames(tx, scope, keys);
    const changes: WeaknessChangeInput[] = estimates.map((e) => {
      const t = transitions
        .filter((h) => h.seriesKey === e.seriesKey && h.toState === e.state)
        .at(-1);
      const min = weaknessDef(e.seriesKey)?.minEvidence ?? null;
      const ev = isObj(t?.evidence) ? t.evidence : null;
      return {
        seriesKey: e.seriesKey,
        label: seriesLabel(e.seriesKey, names.get(e.seriesKey)),
        state: e.state,
        sufficient: min != null && e.nUnweighted >= min,
        chances: e.nUnweighted,
        transition: t
          ? {
              toState: t.toState,
              pChange: t.pChange,
              rateBefore: t.rateBefore,
              rateAfter: t.rateAfter,
              at: t.atPlayedAt,
              counts:
                ev && fin(ev.k) && fin(ev.n) && ev.k >= 0 && ev.n >= 0
                  ? { events: ev.k, chances: ev.n }
                  : null,
            }
          : null,
      };
    });

    // Opening lines: each game's score and expected score from its metrics row.
    const openingRows = await tx
      .select({
        lineKey: gameOpenings.lineKey,
        lineName: gameOpenings.lineName,
        color: gameOpenings.color,
        kind: gameOpenings.lineKind,
        score: sql<number | null>`(${gameMetrics.metrics}->>'score')::float8`,
        expected: sql<number | null>`(${gameMetrics.metrics}->>'expectedScore')::float8`,
      })
      .from(gameOpenings)
      .innerJoin(gameMetrics, eq(gameMetrics.gameId, gameOpenings.gameId))
      .innerJoin(games, eq(games.id, gameMetrics.gameId))
      .where(
        and(
          inGames,
          eq(gameOpenings.accountId, scope.accountId),
          eq(gameOpenings.timeClass, scope.timeClass),
          isNotNull(gameOpenings.lineKey),
          isNotNull(gameOpenings.lineKind),
          sql`jsonb_typeof(${gameMetrics.metrics}->'score') = 'number'`,
          sql`jsonb_typeof(${gameMetrics.metrics}->'expectedScore') = 'number'`,
        ),
      );
    const verdicts = await tx
      .select({
        lineKey: openingLines.lineKey,
        color: openingLines.color,
        kind: openingLines.kind,
        verdict: openingLines.verdict,
      })
      .from(openingLines)
      .where(
        and(
          eq(openingLines.accountId, scope.accountId),
          eq(openingLines.timeClass, scope.timeClass),
        ),
      );
    const lines = new Map<string, OpeningInput>();
    for (const r of openingRows) {
      if (!r.lineKey || !r.kind || r.score == null || r.expected == null) continue;
      const id = `${r.color}:${r.kind}:${r.lineKey}`;
      const line = lines.get(id) ?? {
        lineKey: r.lineKey,
        name: r.lineName ?? r.lineKey,
        color: r.color,
        kind: r.kind,
        verdict:
          verdicts.find((v) => v.lineKey === r.lineKey && v.color === r.color && v.kind === r.kind)
            ?.verdict ?? null,
        games: [],
      };
      if (r.lineName && line.name === line.lineKey) line.name = r.lineName;
      line.games.push({ score: r.score, expected: r.expected });
      lines.set(id, line);
    }
    const judged = [...lines.values()].filter((l) => l.games.length >= MIN_OPENING_GAMES).length;

    // Strength plies where the player found the engine's move (Rust `extract_strength_plies`).
    const spotRows = (await tx.execute(sql`
      select s.game_id, s.played_at, s.kind, s.ply, m.fen_before, m.san, m.uci, m.best_uci,
        m.win_before, m.win_after, m.time_spent, f.difficulty_status, f.p_best, f.is_critical,
        f.multipv_gap, pm.san as prev_san, pm.judgement::text as prev_judgement
      from (
        select gm.game_id, gm.played_at, e->>'kind' as kind, (e->>'ply')::int as ply
        from game_metrics gm
        cross join lateral jsonb_array_elements(
          case when jsonb_typeof(gm.metrics->'strengths') = 'array' then gm.metrics->'strengths'
          else '[]'::jsonb end) e
        join games g on g.id = gm.game_id
        where gm.account_id = ${scope.accountId} and gm.time_class = ${scope.timeClass}::time_class
          ${side === "both" ? sql`` : sql`and g.player_color = ${side}::color`}
          ${range.from ? sql`and gm.played_at >= ${`${range.from}T00:00:00Z`}::timestamptz` : sql``}
          ${range.to ? sql`and gm.played_at < ${dayAfter(range.to).toISOString()}::timestamptz` : sql``}
          and e->>'kind' in ('only_move_found', 'tactic_found')
          and jsonb_typeof(e->'ply') = 'number' and (e->>'ply') ~ '^[0-9]{1,4}$'
      ) s
      join moves m on m.game_id = s.game_id and m.ply = s.ply
      left join move_features f on f.game_id = s.game_id and f.ply = s.ply
      left join moves pm on pm.game_id = s.game_id and pm.ply = s.ply - 1
      order by s.played_at desc, s.ply
      limit 300`)) as unknown as Record<string, unknown>[];
    const candidates: HardSpotCandidate[] = spotRows.flatMap((r): HardSpotCandidate[] => {
      const num = (v: unknown) => (v == null ? null : Number(v));
      const winBefore = num(r.win_before);
      const winAfter = num(r.win_after);
      if (!fin(winBefore) || !fin(winAfter)) return [];
      return [
        {
          gameId: String(r.game_id),
          playedAt: r.played_at instanceof Date ? r.played_at : new Date(String(r.played_at)),
          ply: Number(r.ply),
          kind: r.kind as StrengthKind,
          fen: String(r.fen_before),
          san: String(r.san),
          uci: String(r.uci),
          bestUci: r.best_uci == null ? null : String(r.best_uci),
          winBefore,
          winAfter,
          timeSpent: fin(num(r.time_spent)) ? num(r.time_spent) : null,
          difficultyStatus: r.difficulty_status == null ? null : String(r.difficulty_status),
          pBest: fin(num(r.p_best)) ? num(r.p_best) : null,
          isCritical: typeof r.is_critical === "boolean" ? r.is_critical : null,
          multipvGap: fin(num(r.multipv_gap)) ? num(r.multipv_gap) : null,
          prevSan: r.prev_san == null ? null : String(r.prev_san),
          prevJudgement: r.prev_judgement == null ? null : String(r.prev_judgement),
        },
      ];
    });
    const spots = pickHardSpots(candidates);

    // The newest report input (ruling R2), for the strengths the report and coach name.
    const [run] = await tx
      .select({ input: reportRuns.input, createdAt: reportRuns.createdAt })
      .from(reportRuns)
      .where(
        and(
          eq(reportRuns.accountId, scope.accountId),
          eq(reportRuns.timeClass, scope.timeClass),
          isNotNull(reportRuns.input),
        ),
      )
      .orderBy(desc(reportRuns.createdAt), desc(reportRuns.id))
      .limit(1);

    const option = scope.options.find(
      (o) => o.accountId === scope.accountId && o.timeClass === scope.timeClass,
    );
    return {
      side,
      range,
      analysedGames: filtered ? Number(selected?.n ?? 0) : (option?.analysedGames ?? rows.length),
      above,
      bySide,
      improving: {
        weaknesses: improvingWeaknesses(changes),
        kpis: above.mode === "own" ? null : own,
      },
      openings: {
        lines: workingOpenings([...lines.values()]),
        promising: promisingOpenings([...lines.values()]),
        judged,
      },
      hardSpots: {
        ...spots,
        notes: Object.fromEntries(
          spots.spots.map((s) => [spotKey(s), keepDoingNote(s.kind, s.timeSpent)]),
        ),
      },
      report: run ? { at: run.createdAt, strengths: reportStrengths(run.input) } : null,
    };
  });
  if (!model) return null;
  return { ...model, difficulty: await difficultyStatus(db, userId, scope) };
}
