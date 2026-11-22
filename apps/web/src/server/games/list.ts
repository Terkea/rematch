import { type Db, games, moves } from "@ci/db";
import { and, asc, desc, eq, gte, inArray, lt, type SQL, sql } from "drizzle-orm";
import { readOnly } from "./read-only";
import { playerResult } from "./result";
import { scopedAccountIds } from "./scope";
import {
  type GameFilters,
  type GameListQuery,
  type GamePage,
  type GameRow,
  PAGE_SIZE,
  type TimeClass,
} from "./types";

/** The start of the UTC day after `day` (`YYYY-MM-DD`), so `to` includes its whole day. */
function nextDay(day: string): Date {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + 1);
  return d;
}

const playerWon = sql`((${games.result} = '1-0' and ${games.playerColor} = 'white') or (${games.result} = '0-1' and ${games.playerColor} = 'black'))`;

function filterConditions(ids: string[], f: GameFilters): SQL {
  const conds: SQL[] = [inArray(games.accountId, ids)];
  if (f.timeClass) conds.push(eq(games.timeClass, f.timeClass));
  if (f.colour) conds.push(eq(games.playerColor, f.colour));
  if (f.from) conds.push(gte(games.playedAt, new Date(`${f.from}T00:00:00Z`)));
  if (f.to) conds.push(lt(games.playedAt, nextDay(f.to)));
  if (f.result === "draw") conds.push(eq(games.result, "1/2-1/2"));
  if (f.result === "win") conds.push(playerWon);
  if (f.result === "loss")
    conds.push(sql`(${games.result} in ('1-0', '0-1') and not ${playerWon})`);
  return and(...conds) as SQL;
}

/**
 * How many of the user's games match `filters`, scoped like `listGames` and in the same READ ONLY
 * transaction style, with no rows and no moves read (the explorer's "widen it" counts).
 */
export function countGames(db: Db, userId: string, filters: GameFilters): Promise<number> {
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, filters.accountId);
    if (ids.length === 0) return 0;
    const [row] = await tx
      .select({ total: sql<number>`count(*)::int` })
      .from(games)
      .where(filterConditions(ids, filters));
    return row?.total ?? 0;
  });
}

/**
 * One page of the user's games (ADR 0046, 0048), scoped to their linked accounts and read in a
 * READ ONLY transaction. A page past the end clamps to the last page. Accuracy and judgement
 * counts are set only for finished analyses, and the counts cover the player's own moves.
 */
export function listGames(db: Db, userId: string, q: GameListQuery): Promise<GamePage> {
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, q.filters.accountId);
    if (ids.length === 0) return { rows: [], total: 0, page: 1, pageCount: 1 };
    const where = filterConditions(ids, q.filters);

    const [totalRow] = await tx
      .select({ total: sql<number>`count(*)::int` })
      .from(games)
      .where(where);
    const total = totalRow?.total ?? 0;
    const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
    const page = Math.min(Math.max(1, Math.trunc(q.page) || 1), pageCount);

    const done = sql`${games.analysisStatus} = 'done'`;
    const accuracy = sql<number | null>`case when ${done} then
      case when ${games.playerColor} = 'white' then ${games.whiteAccuracy} else ${games.blackAccuracy} end
    end`;
    const opponentRating = sql<number | null>`case when ${games.playerColor} = 'white'
      then ${games.blackRating} else ${games.whiteRating} end`;
    const dir = sql.raw(q.sort.dir === "asc" ? "asc" : "desc");
    const ordered = (first: SQL): SQL[] =>
      q.sort.key === "played"
        ? [q.sort.dir === "asc" ? asc(games.playedAt) : desc(games.playedAt), asc(games.id)]
        : [sql`${first} ${dir} nulls last`, desc(games.playedAt), asc(games.id)];

    // Except when sorting by blunders, the page is chosen first, so the moves aggregate below
    // covers only its rows and the cost of a page does not grow with the player's history.
    let pageIds: string[] | null = null;
    if (q.sort.key !== "blunders") {
      const idRows = await tx
        .select({ id: games.id })
        .from(games)
        .where(where)
        .orderBy(...ordered(q.sort.key === "accuracy" ? accuracy : opponentRating))
        .limit(PAGE_SIZE)
        .offset((page - 1) * PAGE_SIZE);
      pageIds = idRows.map((r) => r.id);
      if (pageIds.length === 0) return { rows: [], total, page, pageCount };
    }
    const scope = pageIds ? and(where, inArray(games.id, pageIds)) : where;

    // One grouped pass over the chosen games' moves: all plies, and the player's judgements.
    const mine = sql`${moves.color} = ${games.playerColor}`;
    const mv = tx
      .select({
        gameId: moves.gameId,
        plies: sql<number>`count(*)::int`.as("plies"),
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
      .where(scope)
      .groupBy(moves.gameId)
      .as("mv");
    const blunders = sql<number | null>`case when ${done} then coalesce(${mv.blunders}, 0) end`;
    const first =
      q.sort.key === "accuracy" ? accuracy : q.sort.key === "blunders" ? blunders : opponentRating;

    const rows = await tx
      .select({
        id: games.id,
        accountId: games.accountId,
        platform: games.platform,
        playedAt: games.playedAt,
        timeClass: games.timeClass,
        baseSeconds: games.baseSeconds,
        incrementSeconds: games.incrementSeconds,
        rated: games.rated,
        playerColor: games.playerColor,
        whiteName: games.whiteName,
        blackName: games.blackName,
        whiteRating: games.whiteRating,
        blackRating: games.blackRating,
        result: games.result,
        termination: games.termination,
        eco: games.eco,
        openingName: games.openingName,
        analysisStatus: games.analysisStatus,
        accuracy,
        plies: mv.plies,
        blunders: mv.blunders,
        mistakes: mv.mistakes,
        inaccuracies: mv.inaccuracies,
      })
      .from(games)
      .leftJoin(mv, eq(mv.gameId, games.id))
      .where(scope)
      .orderBy(...ordered(first))
      .limit(PAGE_SIZE)
      .offset(pageIds ? 0 : (page - 1) * PAGE_SIZE);

    const out: GameRow[] = rows.map((r) => {
      const white = r.playerColor === "white";
      const isDone = r.analysisStatus === "done";
      return {
        id: r.id,
        accountId: r.accountId,
        platform: r.platform,
        playedAt: r.playedAt,
        timeClass: r.timeClass as TimeClass,
        baseSeconds: r.baseSeconds,
        incrementSeconds: r.incrementSeconds,
        rated: r.rated,
        playerColor: r.playerColor,
        player: {
          name: white ? r.whiteName : r.blackName,
          rating: white ? r.whiteRating : r.blackRating,
        },
        opponent: {
          name: white ? r.blackName : r.whiteName,
          rating: white ? r.blackRating : r.whiteRating,
        },
        result: playerResult(r.result, r.playerColor),
        termination: r.termination,
        eco: r.eco,
        openingName: r.openingName,
        analysisStatus: r.analysisStatus,
        accuracy: isDone && r.accuracy !== null ? Number(r.accuracy) : null,
        counts: isDone
          ? {
              blunders: r.blunders ?? 0,
              mistakes: r.mistakes ?? 0,
              inaccuracies: r.inaccuracies ?? 0,
            }
          : null,
        plies: r.plies ?? null,
      };
    });
    return { rows: out, total, page, pageCount };
  });
}
