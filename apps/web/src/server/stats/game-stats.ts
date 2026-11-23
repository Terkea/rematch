import { type Db, games, moves } from "@ci/db";
import { gameMetrics } from "@ci/db/generated";
import { and, desc, eq, gte, inArray, lt, type SQL, sql } from "drizzle-orm";
import { formatCondition, listFormats } from "../filters/formats";
import type { Color, StatsFilters } from "../filters/params";
import type { Period } from "../filters/presets";
import { readOnly } from "../games/read-only";
import { playerResult } from "../games/result";
import { scopedAccountIds } from "../games/scope";
import type { TimeClass } from "../games/types";
import { type Estimate, estimateMean, estimateRate } from "./estimate";

// Game-level statistics for any filter and period: n, accuracy, judgements per game, score with
// W-D-L, phase accuracy, overall and per colour. Honesty rules: every number carries its n, an
// unknown value is dropped (never read as 0), non-finite values are dropped and not counted, and
// below `KPI_MIN_GAMES` an estimate is "insufficient" with the games still needed.

/**
 * `game_metrics` version whose `accuracyByPhase` is lila's game accuracy over the phase (the
 * fixed formula, ADR-FIX-C-1; Rust `PLAYER_GAME_METRICS_VERSION`). Older rows are left out.
 */
export const PHASE_METRICS_VERSION = 3;

export const PHASES = ["opening", "middlegame", "endgame"] as const;
export type Phase = (typeof PHASES)[number];

/** One game in scope, from the player's side. Analysis values are null unless analysis is done. */
export type GameStatRow = {
  id: string;
  playedAt: Date;
  color: "white" | "black";
  result: "win" | "draw" | "loss" | "unfinished";
  analysed: boolean;
  accuracy: number | null;
  blunders: number | null;
  mistakes: number | null;
  inaccuracies: number | null;
  phaseAccuracy: Record<Phase, number | null>;
};

export type ScoreStats = {
  /** Finished games (an unfinished game has no result). */
  n: number;
  wins: number;
  draws: number;
  losses: number;
  /** Points per game (a draw is half) with a Wilson 80% interval. */
  score: Estimate;
};

export type SideStats = {
  /** Games in scope. */
  n: number;
  /** Games whose analysis is done. */
  analysed: number;
  score: ScoreStats;
  /** Mean player accuracy (0-100) with a bootstrap 80% interval. */
  accuracy: Estimate;
  perGame: { blunders: Estimate; mistakes: Estimate; inaccuracies: Estimate };
  /** Only games with a stored current-version phase accuracy count. */
  phaseAccuracy: Record<Phase, Estimate>;
};

export type GameStats = SideStats & { byColor: Record<"white" | "black", SideStats> };

export type GameStatsScope = {
  timeClass: TimeClass | null;
  fmt: string | null;
  color: Color;
  period: Period;
};

const PCT = { min: 0, max: 100 };

/** The scope of the filters' own date range (no comparison). */
export function scopeOfFilters(f: StatsFilters): GameStatsScope {
  return {
    timeClass: f.tc,
    fmt: f.fmt,
    color: f.color,
    period: { kind: "days", from: f.from, to: f.to },
  };
}

/** The scope of the filters with one comparison side's period. */
export function scopeWithPeriod(f: StatsFilters, period: Period): GameStatsScope {
  return { ...scopeOfFilters(f), period };
}

const finite = (xs: (number | null)[]): number[] =>
  xs.filter((x): x is number => x !== null && Number.isFinite(x));

/** The statistics of some rows (pure; `seed` fixes the bootstrap). */
export function summarise(rows: readonly GameStatRow[], seed: string): SideStats {
  const done = rows.filter((r) => r.analysed);
  const wins = rows.filter((r) => r.result === "win").length;
  const draws = rows.filter((r) => r.result === "draw").length;
  const losses = rows.filter((r) => r.result === "loss").length;
  const finished = wins + draws + losses;
  const mean = (key: "accuracy" | "blunders" | "mistakes" | "inaccuracies", domain?: typeof PCT) =>
    estimateMean(finite(done.map((r) => r[key])), `${seed}:${key}`, ...(domain ? [domain] : []));
  const phase = (p: Phase) =>
    estimateMean(finite(done.map((r) => r.phaseAccuracy[p])), `${seed}:phase:${p}`, PCT);
  return {
    n: rows.length,
    analysed: done.length,
    score: { n: finished, wins, draws, losses, score: estimateRate(wins + draws / 2, finished) },
    accuracy: mean("accuracy", PCT),
    perGame: {
      blunders: mean("blunders"),
      mistakes: mean("mistakes"),
      inaccuracies: mean("inaccuracies"),
    },
    phaseAccuracy: {
      opening: phase("opening"),
      middlegame: phase("middlegame"),
      endgame: phase("endgame"),
    },
  };
}

/** Overall and per-colour statistics of some rows. */
export function summariseByColor(rows: readonly GameStatRow[], seed: string): GameStats {
  return {
    ...summarise(rows, seed),
    byColor: {
      white: summarise(
        rows.filter((r) => r.color === "white"),
        `${seed}:white`,
      ),
      black: summarise(
        rows.filter((r) => r.color === "black"),
        `${seed}:black`,
      ),
    },
  };
}

function nextDay(day: string): Date {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + 1);
  return d;
}

const num = (v: unknown): number | null => {
  if (v === null || v === undefined) return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
};

/**
 * The user's games in `scope`, newest first, one row each, read in a READ ONLY transaction and
 * limited to their linked accounts (narrowed to `accountId` when it is one of them). Judgement
 * counts cover the player's own moves of finished analyses; phase accuracy comes only from
 * `game_metrics` rows of `PHASE_METRICS_VERSION`.
 */
export function gameStatRows(
  db: Db,
  userId: string,
  scope: GameStatsScope,
  opts: { accountId?: string | null } = {},
): Promise<GameStatRow[]> {
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, opts.accountId ?? null);
    if (ids.length === 0) return [];
    const conds: SQL[] = [inArray(games.accountId, ids)];
    if (scope.timeClass) {
      conds.push(eq(games.timeClass, scope.timeClass));
      if (scope.fmt !== null) {
        const options = await listFormats(tx, ids, scope.timeClass);
        const cond = formatCondition(scope.fmt, options);
        if (cond) conds.push(cond);
      }
    }
    if (scope.color !== "both") conds.push(eq(games.playerColor, scope.color));
    const p = scope.period;
    if (p.kind === "days") {
      if (p.from) conds.push(gte(games.playedAt, new Date(`${p.from}T00:00:00Z`)));
      if (p.to) conds.push(lt(games.playedAt, nextDay(p.to)));
    }
    const where = and(...conds) as SQL;

    let picked = tx
      .select({ id: games.id })
      .from(games)
      .where(where)
      .orderBy(desc(games.playedAt), desc(games.id))
      .$dynamic();
    if (p.kind === "games") {
      const take = Math.max(0, Math.trunc(p.take) || 0);
      const skip = Math.max(0, Math.trunc(p.skip) || 0);
      if (take === 0) return [];
      picked = picked.limit(take).offset(skip);
    }
    const inScope = picked.as("in_scope");

    const mine = sql`${moves.color} = ${games.playerColor}`;
    const mv = tx
      .select({
        gameId: moves.gameId,
        blunders:
          sql<number>`(count(*) filter (where ${mine} and ${moves.judgement} = 'blunder'))::int`.as(
            "blunders",
          ),
        mistakes:
          sql<number>`(count(*) filter (where ${mine} and ${moves.judgement} = 'mistake'))::int`.as(
            "mistakes",
          ),
        inaccuracies:
          sql<number>`(count(*) filter (where ${mine} and ${moves.judgement} = 'inaccuracy'))::int`.as(
            "inaccuracies",
          ),
      })
      .from(moves)
      .innerJoin(games, eq(games.id, moves.gameId))
      .innerJoin(inScope, eq(inScope.id, moves.gameId))
      .groupBy(moves.gameId)
      .as("mv");

    const phase = (key: Phase) =>
      sql<number | null>`(${gameMetrics.metrics} -> 'accuracyByPhase' ->> ${key})::float8`;
    const rows = await tx
      .select({
        id: games.id,
        playedAt: games.playedAt,
        color: games.playerColor,
        result: games.result,
        status: games.analysisStatus,
        whiteAccuracy: games.whiteAccuracy,
        blackAccuracy: games.blackAccuracy,
        blunders: mv.blunders,
        mistakes: mv.mistakes,
        inaccuracies: mv.inaccuracies,
        opening: phase("opening"),
        middlegame: phase("middlegame"),
        endgame: phase("endgame"),
      })
      .from(games)
      .innerJoin(inScope, eq(inScope.id, games.id))
      .leftJoin(mv, eq(mv.gameId, games.id))
      .leftJoin(
        gameMetrics,
        and(
          eq(gameMetrics.gameId, games.id),
          eq(gameMetrics.metricsVersion, PHASE_METRICS_VERSION),
        ),
      )
      .orderBy(desc(games.playedAt), desc(games.id));

    return rows.map((r): GameStatRow => {
      const analysed = r.status === "done";
      const own = r.color === "white" ? r.whiteAccuracy : r.blackAccuracy;
      const count = (v: number | null) => (analysed ? (num(v) ?? 0) : null);
      const ph = (v: unknown) => (analysed ? num(v) : null);
      return {
        id: r.id,
        playedAt: r.playedAt,
        color: r.color,
        result: playerResult(r.result, r.color),
        analysed,
        accuracy: analysed ? num(own) : null,
        blunders: count(r.blunders),
        mistakes: count(r.mistakes),
        inaccuracies: count(r.inaccuracies),
        phaseAccuracy: {
          opening: ph(r.opening),
          middlegame: ph(r.middlegame),
          endgame: ph(r.endgame),
        },
      };
    });
  });
}

/** `summariseByColor` of `gameStatRows`, seeded by the user and scope so a page is stable. */
export async function gameStats(
  db: Db,
  userId: string,
  scope: GameStatsScope,
  opts: { accountId?: string | null } = {},
): Promise<GameStats> {
  const rows = await gameStatRows(db, userId, scope, opts);
  return summariseByColor(rows, `game-stats:${JSON.stringify(scope)}`);
}
