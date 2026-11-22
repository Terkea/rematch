import type { Db } from "@ci/db";
import {
  gameOpenings,
  reportRuns,
  weaknessEstimates,
  weaknessSeriesPoint,
  weaknessStateHistory,
} from "@ci/db/generated";
import { and, asc, desc, eq, gte, inArray, isNotNull, sql } from "drizzle-orm";
import { readOnly } from "../games";
import { wilson } from "../stats";
import { contractId, seriesArea, seriesLabel, weaknessDef } from "./catalogue";
import { ownsScope } from "./scope";
import type {
  CoachScope,
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

type EstimateRow = typeof weaknessEstimates.$inferSelect;
type HistoryRow = typeof weaknessStateHistory.$inferSelect;

const day = (d: Date) => d.toISOString().slice(0, 10);

const inScope = (scope: CoachScope) => ({
  estimates: and(
    eq(weaknessEstimates.accountId, scope.accountId),
    eq(weaknessEstimates.timeClass, scope.timeClass),
    // The colour split's per-side rows are not shown yet: these pages read every game.
    eq(weaknessEstimates.side, "both"),
  ),
  history: and(
    eq(weaknessStateHistory.accountId, scope.accountId),
    eq(weaknessStateHistory.timeClass, scope.timeClass),
  ),
});

/** Opening names by series key (`opening:<line_key>`), from the scope's `game_openings`. */
export async function openingNames(
  tx: Db,
  scope: CoachScope,
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
  scope: CoachScope,
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
  return {
    gameId: v.gameId,
    ply: v.ply,
    playedAt: v.playedAt,
    fen: v.fen,
    played,
    best: moveRef(v.best),
    bestPv: strings(v.bestPv),
    refutationPv: strings(v.refutationPv),
    tags: strings(v.tags),
    winBefore: v.winBefore,
    winAfter: v.winAfter,
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
      exemplars: await exemplarsOf(tx, scope, seriesKey),
    };
  });
}
