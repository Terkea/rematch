import type { Db } from "@ci/db";
import { games, weaknessEstimates, weaknessSeriesPoint } from "@ci/db/generated";
import { and, asc, desc, eq, gt, inArray } from "drizzle-orm";
import { readOnly, scopedAccountIds } from "../games";
import type { TimeClass } from "../games/types";
import { seriesLabel, weaknessDef } from "../profile/catalogue";
import type { Prior } from "../stats";
import { type GameStatRow, gameStatRows } from "../stats/game-stats";
import {
  computeTrajectory,
  type RatedGame,
  type Trajectory,
  type TrajectoryWeaknessInput,
} from "./model";

// The trajectory's reads (F6, ADR 0088). Server code only. Every read runs in one READ ONLY
// transaction and checks the account is the user's, so a foreign scope reads nothing (null).

/** Top weaknesses compared then against now: the claimable ones, by priority. */
export const TRAJECTORY_WEAKNESSES = 3;

/** The engine's prior without a benchmark (`profile-core` `estimate_weakness`). */
const NO_BENCHMARK_PRIOR: Prior = { mu: 0.5, n0: 2 };

/**
 * The prior behind a stored estimate: `alpha = n0 mu + k`, `beta = n0 (1 - mu) + n - k` with the
 * stored (weighted) `k` and `n`, so `n0 = alpha + beta - n` and `mu = (alpha - k) / n0`. Shapes
 * that hold no prior (or a floored one) give the engine's no-benchmark prior.
 */
export function priorOf(e: { k: number; n: number; alpha: number; beta: number }): Prior {
  const n0 = e.alpha + e.beta - e.n;
  const mu = n0 > 0 ? (e.alpha - e.k) / n0 : Number.NaN;
  if (!(n0 > 1e-6) || !Number.isFinite(mu) || mu <= 0 || mu >= 1) return NO_BENCHMARK_PRIOR;
  return { mu, n0 };
}

type OwnRating = { id: string; playedAt: Date; own: number | null };

/**
 * Pre-game ratings (ADR-FIX-C-1): Lichess stores the rating at the start of the game; chess.com
 * stores the rating after it, so the pre-game rating is the one after the previous rated game of
 * the time class (the first game keeps its stored one).
 */
export function preGameRatings(
  platform: "lichess" | "chesscom",
  rows: readonly OwnRating[],
): RatedGame[] {
  const out: RatedGame[] = [];
  let previous: number | null = null;
  for (const r of rows) {
    const pre = platform === "chesscom" ? (previous ?? r.own) : r.own;
    if (pre !== null && Number.isFinite(pre)) out.push({ id: r.id, playedAt: r.playedAt, rating: pre });
    previous = r.own;
  }
  return out;
}

const day = (d: Date) => d.toISOString().slice(0, 10);
const own = (color: "white" | "black", w: number | null, b: number | null) =>
  color === "white" ? w : b;

/** The account × time class trajectory, or null when the account is not the user's. */
export async function loadTrajectory(
  db: Db,
  userId: string,
  scope: { accountId: string; timeClass: TimeClass },
): Promise<Trajectory | null> {
  const read = await readOnly(db, async (tx) => {
    const owned = await scopedAccountIds(tx, userId, scope.accountId);
    if (owned.length === 0) return null;
    const rows = await tx
      .select({
        id: games.id,
        playedAt: games.playedAt,
        platform: games.platform,
        color: games.playerColor,
        white: games.whiteRating,
        black: games.blackRating,
      })
      .from(games)
      .where(
        and(
          eq(games.accountId, scope.accountId),
          eq(games.timeClass, scope.timeClass),
          eq(games.rated, true),
        ),
      )
      .orderBy(asc(games.playedAt), asc(games.id));
    const platform = rows[0]?.platform ?? "chesscom";
    const ratings = rows.map((r) => ({
      id: r.id,
      playedAt: r.playedAt,
      own: own(r.color, r.white, r.black),
    }));
    const latest = [...ratings].reverse().find((r) => r.own !== null);
    const estimates = await tx
      .select({
        seriesKey: weaknessEstimates.seriesKey,
        k: weaknessEstimates.k,
        n: weaknessEstimates.n,
        alpha: weaknessEstimates.alpha,
        beta: weaknessEstimates.beta,
      })
      .from(weaknessEstimates)
      .where(
        and(
          eq(weaknessEstimates.accountId, scope.accountId),
          eq(weaknessEstimates.timeClass, scope.timeClass),
          eq(weaknessEstimates.side, "both"),
          gt(weaknessEstimates.priority, 0),
        ),
      )
      .orderBy(desc(weaknessEstimates.priority), asc(weaknessEstimates.seriesKey));
    const top = estimates.filter((e) => weaknessDef(e.seriesKey)).slice(0, TRAJECTORY_WEAKNESSES);
    const points =
      top.length === 0
        ? []
        : await tx
            .select({
              seriesKey: weaknessSeriesPoint.seriesKey,
              gameId: weaknessSeriesPoint.gameId,
              events: weaknessSeriesPoint.events,
              opportunities: weaknessSeriesPoint.opportunities,
            })
            .from(weaknessSeriesPoint)
            .where(
              and(
                eq(weaknessSeriesPoint.accountId, scope.accountId),
                eq(weaknessSeriesPoint.timeClass, scope.timeClass),
                inArray(
                  weaknessSeriesPoint.seriesKey,
                  top.map((e) => e.seriesKey),
                ),
              ),
            );
    return { platform, ratings, latest, top, points };
  });
  if (!read) return null;

  const rated = preGameRatings(read.platform, read.ratings);
  const weaknesses: TrajectoryWeaknessInput[] = read.top.map((e) => {
    const map = new Map<string, { events: number; opportunities: number }>();
    for (const p of read.points) {
      if (p.seriesKey === e.seriesKey) {
        map.set(p.gameId, { events: p.events, opportunities: p.opportunities });
      }
    }
    return { seriesKey: e.seriesKey, label: seriesLabel(e.seriesKey), prior: priorOf(e), points: map };
  });

  const current =
    read.latest && read.latest.own !== null
      ? { rating: read.latest.own, at: read.latest.playedAt }
      : null;
  const seed = `trajectory:${scope.accountId}:${scope.timeClass}`;
  // The windows depend on the ratings alone, so find them first and read the game-level stats of
  // their two day spans only (never the whole history).
  const shape = computeTrajectory({ games: rated, current, stats: [], weaknesses: [], seed });
  if (shape.kind === "not_enough_games") return shape;
  const span = (w: { from: Date; to: Date }) =>
    gameStatRows(
      db,
      userId,
      {
        timeClass: scope.timeClass,
        fmt: null,
        color: "both",
        period: { kind: "days", from: day(w.from), to: day(w.to) },
      },
      { accountId: scope.accountId },
    );
  const [a, b] = await Promise.all([span(shape.then), span(shape.now)]);
  const stats: GameStatRow[] = [...a, ...b];
  return computeTrajectory({
    games: rated,
    current,
    stats,
    weaknesses,
    seed,
  });
}
