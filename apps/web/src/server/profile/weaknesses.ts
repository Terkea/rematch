import { fenKey } from "@ci/chess-core";
import type { Db } from "@ci/db";
import {
  gameMetrics,
  gameOpenings,
  games,
  moves,
  positionEvals,
  reportRuns,
  weaknessEstimates,
  weaknessSeriesPoint,
  weaknessStateHistory,
} from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, asc, desc, eq, gte, inArray, isNotNull, sql } from "drizzle-orm";
import { mateChange } from "../../lib/explain";
import { readOnly } from "../games";
import { wilson } from "../stats";
import { contractId, isPlayerVisibleTag, seriesArea, seriesLabel, weaknessDef } from "./catalogue";
import { ownsScope } from "./scope";
import type {
  CoachScope,
  Example,
  Exemplar,
  TimelineSeries,
  WeaknessDetail,
  WeaknessState,
  WeaknessSummary,
} from "./types";
import { ESTIMATE_WINDOW_DAYS, ESTIMATE_WINDOW_TEXT } from "./window";

export { ESTIMATE_WINDOW_DAYS, ESTIMATE_WINDOW_TEXT };

/**
 * Weeks of `weakness_series_point` a detail page shows at most: every ISO week the estimate's
 * window touches (183 days span up to 28), so the weekly list adds up to the estimate's counts.
 */
export const WEEKLY_WEEKS = 28;

const DAY_MS = 86_400_000;

/** What a read of stored rows needs of a scope: the account, the time class and the side. */
export type ScopeKey = Pick<CoachScope, "accountId" | "timeClass" | "side">;

type EstimateRow = typeof weaknessEstimates.$inferSelect;
type HistoryRow = typeof weaknessStateHistory.$inferSelect;

const day = (d: Date) => d.toISOString().slice(0, 10);

/**
 * A series point of one of the side's games: every point for "both", else only the points of the
 * games the player had that colour in (how build-profile splits them, ADR 0082).
 */
export function pointOnSide(scope: ScopeKey) {
  const side = scope.side ?? "both";
  if (side === "both") return undefined;
  return sql`exists (select 1 from ${games} where ${games.id} = ${weaknessSeriesPoint.gameId}
    and ${games.playerColor} = ${side})`;
}

const inScope = (scope: ScopeKey) => ({
  estimates: and(
    eq(weaknessEstimates.accountId, scope.accountId),
    eq(weaknessEstimates.timeClass, scope.timeClass),
    // The colour filter's side (ADR 0082); every game unless the scope names a colour.
    eq(weaknessEstimates.side, scope.side ?? "both"),
  ),
  history: and(
    eq(weaknessStateHistory.accountId, scope.accountId),
    eq(weaknessStateHistory.timeClass, scope.timeClass),
  ),
});

/** Opening names by series key (`opening:<line_key>`), from the scope's `game_openings`. */
export async function openingNames(
  tx: Db,
  scope: ScopeKey,
  seriesKeys: string[],
): Promise<Map<string, string>> {
  const lineKeys = seriesKeys
    .filter((k) => k.startsWith("opening:"))
    .map((k) => k.slice("opening:".length));
  if (lineKeys.length === 0) return new Map();
  const rows = await tx
    .selectDistinctOn([gameOpenings.lineKey], {
      lineKey: gameOpenings.lineKey,
      lineName: gameOpenings.lineName,
    })
    .from(gameOpenings)
    .where(
      and(
        eq(gameOpenings.accountId, scope.accountId),
        eq(gameOpenings.timeClass, scope.timeClass),
        inArray(gameOpenings.lineKey, lineKeys),
        isNotNull(gameOpenings.lineName),
      ),
    )
    .orderBy(gameOpenings.lineKey, gameOpenings.lineName);
  return new Map(rows.map((r) => [`opening:${r.lineKey}`, r.lineName ?? ""]));
}

function evidence(row: EstimateRow): WeaknessSummary["evidence"] {
  const minEvidence = weaknessDef(row.seriesKey)?.minEvidence ?? null;
  const chances = row.nUnweighted;
  // R3: sufficient when n_unweighted >= min_evidence; a series without a minimum never is.
  const sufficient = minEvidence != null && chances >= minEvidence;
  return {
    events: row.kUnweighted,
    chances,
    minEvidence,
    sufficient,
    needMore: minEvidence == null || sufficient ? null : Math.ceil(minEvidence - chances),
  };
}

function summary(
  row: EstimateRow,
  onset: HistoryRow | undefined,
  names: Map<string, string>,
): WeaknessSummary {
  return {
    seriesKey: row.seriesKey,
    contractId: contractId(row.seriesKey),
    label: seriesLabel(row.seriesKey, names.get(row.seriesKey)),
    area: seriesArea(row.seriesKey),
    state: row.state,
    rate: { mean: row.mean, low: row.ciLow, high: row.ciHigh, level: 0.8 },
    evidence: evidence(row),
    band: { median: row.bandMedian, p40: row.bandP40 },
    // Rust stores `excess` (and the band) only with a benchmark; without one the state is
    // `not_an_issue` by construction, which is no claim (bug hunt I1).
    compared: row.bandMedian != null && row.excess != null,
    priority: row.priority,
    confidence: row.confidence,
    pImp: row.pImp,
    updatedAt: row.updatedAt,
    onset: onset?.onsetAt
      ? {
          at: onset.onsetAt,
          low: onset.onsetLow,
          high: onset.onsetHigh,
          gameId: onset.onsetGameId,
        }
      : null,
  };
}

/** The scope's weakness summaries (optionally one series), by priority desc, then label. */
export async function summariesIn(
  tx: Db,
  scope: ScopeKey,
  seriesKey?: string,
): Promise<WeaknessSummary[]> {
  const where = inScope(scope);
  const rows = await tx
    .select()
    .from(weaknessEstimates)
    .where(
      seriesKey == null
        ? where.estimates
        : and(where.estimates, eq(weaknessEstimates.seriesKey, seriesKey)),
    );
  if (rows.length === 0) return [];
  const keys = rows.map((r) => r.seriesKey);
  const onsets = await tx
    .selectDistinctOn([weaknessStateHistory.seriesKey])
    .from(weaknessStateHistory)
    .where(
      and(
        where.history,
        inArray(weaknessStateHistory.seriesKey, keys),
        eq(weaknessStateHistory.kind, "onset"),
        isNotNull(weaknessStateHistory.onsetAt),
      ),
    )
    .orderBy(weaknessStateHistory.seriesKey, desc(weaknessStateHistory.id));
  const onsetOf = new Map(onsets.map((o) => [o.seriesKey, o]));
  const names = await openingNames(tx, scope, keys);
  return rows
    .map((r) => summary(r, onsetOf.get(r.seriesKey), names))
    .sort(
      (a, b) =>
        b.priority - a.priority ||
        (a.label < b.label ? -1 : a.label > b.label ? 1 : 0) ||
        (a.seriesKey < b.seriesKey ? -1 : 1),
    );
}

/** Every `weakness_estimates` row of the scope, by priority desc, then label. */
export async function listWeaknesses(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<WeaknessSummary[]> {
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return [];
    return summariesIn(tx, scope);
  });
}

/**
 * State segments from transition rows in id order: each runs from its row's game to the next
 * row's, the last is open. A row without a game date cannot be placed and is skipped.
 */
export function timelineOf(
  id: string,
  label: string,
  rows: Pick<
    HistoryRow,
    "kind" | "toState" | "atPlayedAt" | "onsetAt" | "onsetLow" | "onsetHigh"
  >[],
): TimelineSeries {
  const transitions = rows.filter(
    (r): r is typeof r & { atPlayedAt: Date } => r.kind === "transition" && r.atPlayedAt != null,
  );
  const segments = transitions.map((r, i) => {
    const next = transitions[i + 1];
    return { state: r.toState, from: day(r.atPlayedAt), to: next ? day(next.atPlayedAt) : null };
  });
  const onset = [...rows]
    .reverse()
    .find((r) => r.kind === "onset" && r.onsetAt && r.onsetLow && r.onsetHigh);
  return {
    id,
    label,
    segments,
    ...(onset?.onsetAt && onset.onsetLow && onset.onsetHigh
      ? {
          onset: { date: day(onset.onsetAt), low: day(onset.onsetLow), high: day(onset.onsetHigh) },
        }
      : {}),
  };
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const isStr = (v: unknown): v is string => typeof v === "string";
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter(isStr) : []);
const moveRef = (v: unknown) =>
  isObj(v) && isStr(v.san) && isStr(v.uci) ? { san: v.san, uci: v.uci } : null;

/** A contract `ExemplarPosition`, or null when the stored JSON does not have its shape. */
function exemplar(v: unknown): Exemplar | null {
  if (!isObj(v)) return null;
  const played = moveRef(v.played);
  if (
    !isStr(v.gameId) ||
    !isNum(v.ply) ||
    !isStr(v.playedAt) ||
    !isStr(v.fen) ||
    !played ||
    !isNum(v.winBefore) ||
    !isNum(v.winAfter)
  ) {
    return null;
  }
  const mate = mateChange(
    v.fen,
    isObj(v.evalBefore) ? v.evalBefore : null,
    isObj(v.evalAfter) ? v.evalAfter : null,
  );
  return {
    gameId: v.gameId,
    ply: v.ply,
    playedAt: v.playedAt,
    fen: v.fen,
    played,
    best: moveRef(v.best),
    bestPv: strings(v.bestPv),
    refutationPv: strings(v.refutationPv),
    tags: strings(v.tags).filter(isPlayerVisibleTag),
    winBefore: v.winBefore,
    winAfter: v.winAfter,
    ...(mate ? { mate } : {}),
  };
}

/** Ruling R2: the exemplars of the newest non-null `report_runs.input` of the scope. */
async function exemplarsOf(
  tx: Db,
  scope: CoachScope,
  seriesKey: string,
): Promise<WeaknessDetail["exemplars"]> {
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
  if (!run) return null;
  const def = weaknessDef(seriesKey);
  const id = def?.contractId ?? contractId(seriesKey);
  const variant = def?.variant ?? null;
  const weaknesses =
    isObj(run.input) && Array.isArray(run.input.weaknesses) ? run.input.weaknesses : [];
  const match = weaknesses.find(
    (w): w is Record<string, unknown> =>
      isObj(w) && id != null && w.id === id && (w.variant ?? null) === variant,
  );
  const items = Array.isArray(match?.exemplars)
    ? match.exemplars.map(exemplar).filter((e): e is Exemplar => e != null)
    : [];
  return { fromReportAt: run.createdAt, items };
}

async function weeklyOf(
  tx: Db,
  scope: CoachScope,
  seriesKey: string,
  from: Date,
): Promise<WeaknessDetail["weekly"]> {
  const week = sql<string>`to_char(${weaknessSeriesPoint.playedAt} at time zone 'UTC', 'IYYY-"W"IW')`;
  const rows = await tx
    .select({
      week,
      events: sql<number>`sum(${weaknessSeriesPoint.events})::int`,
      chances: sql<number>`sum(${weaknessSeriesPoint.opportunities})::int`,
    })
    .from(weaknessSeriesPoint)
    .where(
      and(
        eq(weaknessSeriesPoint.accountId, scope.accountId),
        eq(weaknessSeriesPoint.timeClass, scope.timeClass),
        eq(weaknessSeriesPoint.seriesKey, seriesKey),
        gte(weaknessSeriesPoint.playedAt, from),
        pointOnSide(scope),
      ),
    )
    .groupBy(week)
    .orderBy(desc(week))
    .limit(WEEKLY_WEEKS);
  return rows.reverse().map((r) => {
    const interval = wilson(r.events, r.chances, 0.8);
    return {
      week: r.week,
      events: r.events,
      chances: r.chances,
      rate: interval ? { value: r.events / r.chances, ...interval } : null,
    };
  });
}

/** Example positions a weakness page shows at most. */
export const MAX_EXAMPLES = 3;

/** The SAN of `uci` in `fen`, or null when it is not a legal move there. */
function sanOf(fen: string, uci: string | null): string | null {
  if (!uci || uci.length < 4) return null;
  try {
    return new Chess(fen).move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      promotion: uci[4] ?? "q",
    }).san;
  } catch {
    return null;
  }
}

/**
 * The page's examples (see `WeaknessDetail.examples`). A stored occurrence is a ply from the
 * series' `exemplarPlies` (Rust orders them by win% lost), kept only when it is the player's own
 * move in a game of the scope inside the estimate's window; each game gives its first such ply.
 */
async function examplesOf(
  tx: Db,
  scope: CoachScope,
  seriesKey: string,
  report: Exemplar[],
  from: Date,
): Promise<Example[]> {
  const out: Example[] = [];
  const seen = new Set<string>();
  for (const e of report) {
    if (out.length >= MAX_EXAMPLES) break;
    if (seen.has(e.gameId)) continue;
    seen.add(e.gameId);
    const { mate = null, ...rest } = e;
    out.push({ ...rest, source: "report", judgement: null, mate });
  }
  if (out.length < MAX_EXAMPLES)
    out.push(
      ...(await storedExamples(tx, scope, seriesKey, from, seen, MAX_EXAMPLES - out.length)),
    );
  return withStored(tx, scope, out);
}

/** The scope's stored occurrences, after the report's (see `examplesOf`). */
async function storedExamples(
  tx: Db,
  scope: CoachScope,
  seriesKey: string,
  from: Date,
  seen: Set<string>,
  max: number,
): Promise<Example[]> {
  const out: Example[] = [];
  const plies = sql`(case when jsonb_typeof(${gameMetrics.metrics} -> 'weakness' -> ${seriesKey}::text -> 'exemplarPlies') = 'array'
      then ${gameMetrics.metrics} -> 'weakness' -> ${seriesKey}::text -> 'exemplarPlies' else '[]'::jsonb end)`;
  const rows = await tx
    .select({
      gameId: games.platformGameId,
      playedAt: games.playedAt,
      ply: moves.ply,
      san: moves.san,
      uci: moves.uci,
      fen: moves.fenBefore,
      bestUci: moves.bestUci,
      winBefore: moves.winBefore,
      winAfter: moves.winAfter,
      judgement: moves.judgement,
      rank: sql<number>`p.ord`,
    })
    .from(gameMetrics)
    .innerJoin(games, eq(games.id, gameMetrics.gameId))
    .innerJoin(sql`jsonb_array_elements_text(${plies}) with ordinality as p(ply, ord)`, sql`true`)
    .innerJoin(
      moves,
      and(eq(moves.gameId, games.id), sql`p.ply ~ '^[0-9]{1,4}$' and ${moves.ply} = p.ply::int`),
    )
    .where(
      and(
        eq(gameMetrics.accountId, scope.accountId),
        eq(games.accountId, scope.accountId),
        eq(gameMetrics.timeClass, scope.timeClass),
        gte(games.playedAt, from),
        sql`${moves.color} = ${games.playerColor}`,
        scope.side && scope.side !== "both" ? eq(games.playerColor, scope.side) : undefined,
      ),
    )
    .orderBy(desc(games.playedAt), games.platformGameId, sql`p.ord`)
    .limit(MAX_EXAMPLES * 10);
  for (const r of rows) {
    if (out.length >= max) break;
    if (seen.has(r.gameId)) continue;
    seen.add(r.gameId);
    const bestSan = r.bestUci && r.bestUci !== r.uci ? sanOf(r.fen, r.bestUci) : null;
    out.push({
      gameId: r.gameId,
      ply: r.ply,
      playedAt: r.playedAt.toISOString(),
      fen: r.fen,
      played: { san: r.san, uci: r.uci },
      best: bestSan && r.bestUci ? { san: bestSan, uci: r.bestUci } : null,
      bestPv: [],
      refutationPv: [],
      tags: [],
      winBefore: r.winBefore,
      winAfter: r.winAfter,
      source: "games",
      judgement:
        r.judgement === "inaccuracy" || r.judgement === "mistake" || r.judgement === "blunder"
          ? r.judgement
          : null,
      mate: null,
    });
  }
  return out;
}

/** The position after `uci` in `fen`, or null when it is not a legal move there. */
function fenAfter(fen: string, uci: string): string | null {
  try {
    const chess = new Chess(fen);
    chess.move({ from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] ?? "q" });
    return chess.fen();
  } catch {
    return null;
  }
}

/**
 * What the stored analysis adds to the examples of imported games: the played move's judgement
 * (the report's exemplars carry none) and, where the report gave none, a mate the move gave up
 * or allowed, from `position_evals` under the game's own protocol.
 */
async function withStored(tx: Db, scope: CoachScope, list: Example[]): Promise<Example[]> {
  if (list.length === 0) return list;
  const rows = await tx
    .select({
      gameId: games.platformGameId,
      ply: moves.ply,
      uci: moves.uci,
      judgement: moves.judgement,
      protocol: games.analysisProtocol,
    })
    .from(moves)
    .innerJoin(games, eq(games.id, moves.gameId))
    .where(
      and(
        eq(games.accountId, scope.accountId),
        inArray(games.platformGameId, [...new Set(list.map((e) => e.gameId))]),
        inArray(moves.ply, [...new Set(list.map((e) => e.ply))]),
        sql`${moves.color} = ${games.playerColor}`,
      ),
    );
  const stored = new Map(rows.map((r) => [`${r.gameId}#${r.ply}`, r]));
  const after = new Map(list.map((e) => [e, fenAfter(e.fen, e.played.uci)]));
  const keys = [
    ...new Set(
      list.flatMap((e) => {
        const a = after.get(e);
        return a ? [fenKey(e.fen), fenKey(a)] : [];
      }),
    ),
  ];
  const protocols = [...new Set(rows.flatMap((r) => (r.protocol ? [r.protocol] : [])))];
  const evals = new Map<string, { cp: number | null; mate: number | null }>();
  if (keys.length > 0 && protocols.length > 0) {
    const found = await tx
      .select({
        fenKey: positionEvals.fenKey,
        protocol: positionEvals.protocol,
        cp: positionEvals.scoreCp,
        mate: positionEvals.scoreMate,
      })
      .from(positionEvals)
      .where(and(inArray(positionEvals.protocol, protocols), inArray(positionEvals.fenKey, keys)));
    for (const f of found) evals.set(`${f.protocol}|${f.fenKey}`, { cp: f.cp, mate: f.mate });
  }
  return list.map((e) => {
    const r = stored.get(`${e.gameId}#${e.ply}`);
    if (!r || r.uci !== e.played.uci) return e;
    const a = after.get(e);
    const score = (fen: string) =>
      r.protocol ? evals.get(`${r.protocol}|${fenKey(fen)}`) : undefined;
    const mate = e.mate ?? (a ? mateChange(e.fen, score(e.fen), score(a)) : null);
    return { ...e, judgement: e.judgement ?? r.judgement ?? null, mate };
  });
}

/** A history row's stored `evidence` as counts; null unless both are whole, non-negative numbers. */
function countsOf(v: unknown): { events: number; chances: number } | null {
  if (!isObj(v)) return null;
  const { k, n } = v;
  return isNum(k) && isNum(n) && k >= 0 && n >= 0 && Number.isInteger(k) && Number.isInteger(n)
    ? { events: k, chances: n }
    : null;
}

/** One weakness with its history, timeline, weekly rates and exemplars; null when not found. */
export async function getWeakness(
  db: Db,
  userId: string,
  scope: CoachScope,
  seriesKey: string,
): Promise<WeaknessDetail | null> {
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return null;
    const [base] = await summariesIn(tx, scope, seriesKey);
    if (!base) return null;
    const rows = await tx
      .select()
      .from(weaknessStateHistory)
      .where(and(inScope(scope).history, eq(weaknessStateHistory.seriesKey, seriesKey)))
      .orderBy(asc(weaknessStateHistory.id));
    const window = {
      days: ESTIMATE_WINDOW_DAYS,
      from: new Date(base.updatedAt.getTime() - ESTIMATE_WINDOW_DAYS * DAY_MS),
      to: base.updatedAt,
    };
    const exemplars = await exemplarsOf(tx, scope, seriesKey);
    return {
      ...base,
      window,
      history: rows.map((r) => ({
        kind: r.kind,
        fromState: r.fromState as WeaknessState | null,
        toState: r.toState,
        at: r.atPlayedAt,
        onsetAt: r.onsetAt,
        onsetLow: r.onsetLow,
        onsetHigh: r.onsetHigh,
        pChange: r.pChange,
        rateBefore: r.rateBefore,
        rateAfter: r.rateAfter,
        causeHints: Array.isArray(r.causeHints) ? r.causeHints : [],
        counts: countsOf(r.evidence),
      })),
      timeline: { ...timelineOf(seriesKey, base.label, rows), compared: base.compared },
      weekly: await weeklyOf(tx, scope, seriesKey, window.from),
      exemplars,
      examples: await examplesOf(tx, scope, seriesKey, exemplars?.items ?? [], window.from),
    };
  });
}
