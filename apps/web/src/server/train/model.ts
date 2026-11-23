import { type EngineScore, winPercent } from "@ci/chess-core";
import { Chess } from "chess.js";
import { obviousCapture } from "../profile/strengths";
import { fnv1a, type Interval, mulberry32, wilson } from "../stats";

// The missed-moments trainer's pure rules (ADR 0090). Everything here is unit tested; `store.ts`
// only reads and writes the rows these functions decide on.

// ---- Sources ----

/** The positions drilled, most specific first: a position in several takes the first. */
export const SOURCES = ["W4.missed", "W3.shallow", "W3.deep", "W7.traps", "blunder"] as const;
export type SourceKey = (typeof SOURCES)[number];

export const isSourceKey = (v: unknown): v is SourceKey =>
  typeof v === "string" && (SOURCES as readonly string[]).includes(v);

/** The weakness series a source is (all but `blunder`, which is the stored judgement). */
export const WEAKNESS_SOURCES = SOURCES.filter((s) => s !== "blunder");

export function primarySource(sources: readonly SourceKey[]): SourceKey {
  return SOURCES.find((s) => sources.includes(s)) ?? "blunder";
}

// ---- Grading ----

/** A `position_multipv` line (profile-core `MultipvLine`): the score is the mover's. */
export type MultipvLine = { uci: string; score: EngineScore; pv: string[] };

/** A move at most this many win% points below the best keeps the outcome. */
export const WIN_BAND = 5;

const mateOf = (s: EngineScore): number | null => ("mate" in s ? s.mate : null);

/**
 * The moves that pass (ADR 0090): the engine's move, plus each stored alternative that keeps the
 * outcome. When the best move mates (in `n`), a mate in at most `n + 1`; otherwise at most
 * `WIN_BAND` win% points below the best. The reference is the best move's own line, else the top
 * line. `mateIn` is the best move's mate, or null.
 */
export function acceptedSet(
  bestUci: string,
  lines: readonly MultipvLine[],
): { accepted: string[]; mateIn: number | null } {
  const ref = lines.find((l) => l.uci === bestUci) ?? lines[0];
  const accepted = [bestUci];
  if (!ref) return { accepted, mateIn: null };
  const refMate = mateOf(ref.score);
  const mating = refMate !== null && refMate > 0 ? refMate : null;
  for (const l of lines) {
    if (accepted.includes(l.uci)) continue;
    const m = mateOf(l.score);
    const keeps =
      mating !== null
        ? m !== null && m > 0 && m <= mating + 1
        : winPercent(l.score) >= winPercent(ref.score) - WIN_BAND;
    if (keeps) accepted.push(l.uci);
  }
  return { accepted, mateIn: mating };
}

export const grade = (accepted: readonly string[], uci: string): boolean =>
  accepted.includes(uci.toLowerCase());

/** SAN of `uci` in `fen`, or the UCI itself when it is not legal there. */
export function sanOf(fen: string, uci: string): string {
  try {
    return new Chess(fen).move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      ...(uci.length > 4 ? { promotion: uci.slice(4, 5) } : {}),
    }).san;
  } catch {
    return uci;
  }
}

/**
 * The verdict line, about the move and never the player (Kluger and DeNisi): "Right: Qg7#
 * mates.", "Right: Nf5 is the engine's move.", or "Not this time: the engine's move was Qg7#."
 */
export function feedback(p: {
  correct: boolean;
  fen: string;
  moveUci: string;
  solutionUci: string;
}): string {
  const best = sanOf(p.fen, p.solutionUci);
  if (!p.correct) return `Not this time: the engine's move was ${best}.`;
  const san = sanOf(p.fen, p.moveUci);
  if (san.endsWith("#")) return `Right: ${san} mates.`;
  if (p.moveUci.toLowerCase() === p.solutionUci) return `Right: ${san} is the engine's move.`;
  return `Right: ${san} is one of the engine's good moves here (it chose ${best}).`;
}

// ---- Days (UTC) ----

const DAY_MS = 86_400_000;

export const dayOf = (d: Date): string => d.toISOString().slice(0, 10);

export const addDays = (day: string, n: number): string =>
  dayOf(new Date(Date.parse(`${day}T00:00:00Z`) + n * DAY_MS));

export const daysBetween = (a: string, b: string): number =>
  Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / DAY_MS);

// ---- Scheduling ----

/** Leitner intervals in days: the n-th right answer in a row is due again after the n-th. */
export const INTERVALS = [1, 3, 7, 21, 60] as const;
/** Sure right answers at least `CLEAN_GAP_DAYS` after the previous attempt that retire an item. */
export const RETIRE_PASSES = 3;
export const CLEAN_GAP_DAYS = 7;

export type Confidence = "sure" | "guess";

export type Schedule = {
  streak: number;
  cleanPasses: number;
  dueOn: string | null;
  introducedOn: string | null;
  lastAttemptOn: string | null;
  retired: boolean;
};

/**
 * The schedule after one attempt. Right: the streak grows and the item is due after the next
 * interval; a sure answer at least `CLEAN_GAP_DAYS` after the previous attempt is a clean pass,
 * and `RETIRE_PASSES` of them retire it. A miss resets the streak and the clean passes and makes
 * the item due tomorrow.
 */
export function afterAttempt(
  s: Schedule,
  a: { correct: boolean; confidence: Confidence; today: string },
): Schedule {
  const introducedOn = s.introducedOn ?? a.today;
  if (!a.correct) {
    return {
      streak: 0,
      cleanPasses: 0,
      dueOn: addDays(a.today, 1),
      introducedOn,
      lastAttemptOn: a.today,
      retired: false,
    };
  }
  const streak = s.streak + 1;
  const gap = s.lastAttemptOn ? daysBetween(s.lastAttemptOn, a.today) : 0;
  const clean = a.confidence === "sure" && s.lastAttemptOn !== null && gap >= CLEAN_GAP_DAYS;
  const cleanPasses = s.cleanPasses + (clean ? 1 : 0);
  const interval = INTERVALS[Math.min(streak, INTERVALS.length) - 1] ?? 1;
  return {
    streak,
    cleanPasses,
    dueOn: addDays(a.today, interval),
    introducedOn,
    lastAttemptOn: a.today,
    retired: cleanPasses >= RETIRE_PASSES,
  };
}

// ---- Today's session ----

/** About 10 to 15 minutes. */
export const SESSION_SIZE = 12;
/** Attempts a day at most, so a backlog never piles up into one sitting (two sessions). */
export const DAILY_CAP = 24;
/** New items introduced a day at most. */
export const NEW_PER_DAY = 10;

export type QueueItem = {
  id: string;
  sourceKey: SourceKey;
  sourceKeys: SourceKey[];
  dueOn: string | null;
  introducedOn: string | null;
  heldOut: boolean;
  retired: boolean;
  playedAt: Date;
};

/**
 * Today's items, in order: those due (oldest due first), then new ones (newest game first) while
 * fewer than `NEW_PER_DAY` were introduced today, at most `SESSION_SIZE` and never past
 * `DAILY_CAP` attempts today. Held-out and retired items never come. `source` keeps the items of
 * that source (any of an item's sources). The picked items are interleaved: one source after
 * another in a seeded order, so the motif is never announced by its neighbours.
 */
export function buildSession(
  items: readonly QueueItem[],
  o: {
    today: string;
    attemptedToday: number;
    introducedToday: number;
    seed: string;
    source?: SourceKey;
  },
): string[] {
  const room = Math.max(0, Math.min(SESSION_SIZE, DAILY_CAP - o.attemptedToday));
  if (room === 0) return [];
  const live = items.filter(
    (i) => !i.heldOut && !i.retired && (!o.source || i.sourceKeys.includes(o.source)),
  );
  const due = live
    .filter((i) => i.introducedOn !== null && i.dueOn !== null && i.dueOn <= o.today)
    .sort(
      (a, b) => (a.dueOn as string).localeCompare(b.dueOn as string) || a.id.localeCompare(b.id),
    )
    .slice(0, room);
  const newRoom = Math.max(0, Math.min(NEW_PER_DAY - o.introducedToday, room - due.length));
  const fresh = live
    .filter((i) => i.introducedOn === null)
    .sort((a, b) => b.playedAt.getTime() - a.playedAt.getTime() || a.id.localeCompare(b.id))
    .slice(0, newRoom);
  return interleave([...due, ...fresh], `${o.seed}:${o.today}`).map((i) => i.id);
}

/** Round robin over the sources, each source's items in their order, sources in a seeded order. */
function interleave(list: QueueItem[], seed: string): QueueItem[] {
  const groups = new Map<SourceKey, QueueItem[]>();
  for (const i of list) groups.set(i.sourceKey, [...(groups.get(i.sourceKey) ?? []), i]);
  const rand = mulberry32(fnv1a(seed));
  const order = [...groups.keys()]
    .map((k) => ({ k, r: rand() }))
    .sort((a, b) => a.r - b.r)
    .map((x) => x.k);
  const out: QueueItem[] = [];
  let last: SourceKey | null = null;
  while (out.length < list.length) {
    // The largest group first, never the source just served when another is left.
    const next = order
      .filter((k) => (groups.get(k)?.length ?? 0) > 0)
      .sort((a, b) => (groups.get(b)?.length ?? 0) - (groups.get(a)?.length ?? 0))
      .find((k, _, all) => k !== last || all.length === 1);
    if (!next) break;
    const item = groups.get(next)?.shift();
    if (item) out.push(item);
    last = next;
  }
  return out;
}

// ---- Selection ----

/** A position already decided: the mover was this sure before the move ... */
export const DECIDED_BEFORE = 97;
/** ... and still this sure after it. */
export const DECIDED_AFTER = 90;

export type Candidate = {
  gameId: string;
  ply: number;
  fen: string;
  /** The move played. */
  uci: string;
  bestUci: string | null;
  winBefore: number;
  winAfter: number;
  /** The opponent's previous move (SAN), for the free-capture check. */
  prevSan: string | null;
  sources: SourceKey[];
};

/**
 * Whether a position makes a drill: it has a source, an engine move other than the one played,
 * was not already decided before and after the move, and its answer is not a free capture
 * (`obviousCapture`), unless it is a missed mate.
 */
export function keepCandidate(c: Candidate): boolean {
  if (c.sources.length === 0 || !c.bestUci || c.bestUci === c.uci) return false;
  if (c.winBefore >= DECIDED_BEFORE && c.winAfter >= DECIDED_AFTER) return false;
  if (!c.sources.includes("W4.missed") && obviousCapture(c.fen, c.bestUci, c.prevSan)) return false;
  return true;
}

/** Share of items held out of drilling for the efficacy comparison. */
export const HOLDOUT_PERCENT = 20;

/** Whether a position is in the holdout: a hash of the position, so it is decided once. */
export const isHeldOut = (gameId: string, ply: number): boolean =>
  fnv1a(`holdout:${gameId}:${ply}`) % 100 < HOLDOUT_PERCENT;

// ---- Efficacy ----

/** Fewer chances than this on either side and the panel says "too early to tell". */
export const MIN_CHANCES = 30;

export type Rate = {
  events: number;
  chances: number;
  rate: number | null;
  low: number | null;
  high: number | null;
};

export function rateOf(events: number, chances: number): Rate {
  const ci: Interval | null = wilson(events, chances);
  return {
    events,
    chances,
    rate: chances > 0 ? events / chances : null,
    low: ci?.low ?? null,
    high: ci?.high ?? null,
  };
}

export type Verdict = "too_early" | "lower" | "higher" | "no_clear_change";

/** A claim only when both sides have `MIN_CHANCES` and the 80% intervals separate. */
export function verdictOf(before: Rate, after: Rate): Verdict {
  if (before.chances < MIN_CHANCES || after.chances < MIN_CHANCES) return "too_early";
  const { low: bl, high: bh } = before;
  const { low: al, high: ah } = after;
  if (bl === null || bh === null || al === null || ah === null) return "too_early";
  if (ah < bl) return "lower";
  if (al > bh) return "higher";
  return "no_clear_change";
}

/** The weakness rate per chance in games before the first drill and from it on. */
export function beforeAfter(
  games: readonly { playedAt: Date; events: number; chances: number }[],
  firstDrill: Date | null,
): { before: Rate; after: Rate; verdict: Verdict } {
  let be = 0;
  let bn = 0;
  let ae = 0;
  let an = 0;
  for (const g of games) {
    if (firstDrill && g.playedAt >= firstDrill) {
      ae += g.events;
      an += g.chances;
    } else {
      be += g.events;
      bn += g.chances;
    }
  }
  const before = rateOf(be, bn);
  const after = rateOf(ae, an);
  return { before, after, verdict: verdictOf(before, after) };
}

export type MotifCounts = Record<string, { opportunities: number; misses: number }>;

/**
 * How often the player found a pattern in real games after `since`: the chances at any of the
 * motifs (`missedByMotif` of each game) and how many of them they did not miss.
 */
export function patternFound(
  games: readonly { playedAt: Date; byMotif: MotifCounts }[],
  tags: ReadonlySet<string>,
  since: Date,
): Rate {
  let found = 0;
  let chances = 0;
  for (const g of games) {
    if (g.playedAt < since) continue;
    for (const tag of tags) {
      const c = g.byMotif[tag];
      if (!c || !Number.isFinite(c.opportunities) || !Number.isFinite(c.misses)) continue;
      const n = Math.max(0, c.opportunities);
      chances += n;
      found += Math.max(0, n - Math.min(n, Math.max(0, c.misses)));
    }
  }
  return rateOf(found, chances);
}
