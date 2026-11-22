import { type Db, games, moves } from "@ci/db";
import { and, asc, desc, eq, gte, inArray, lt, type SQL, sql } from "drizzle-orm";
import { readOnly } from "./read-only";
import { playerResult } from "./result";
import { scopedAccountIds } from "./scope";
import {
  type GameListQuery,
  type GamePage,
  type GameRow,
  PAGE_SIZE,
  type TimeClass,
} from "./types";

async function countGames(tx: Db, where: SQL | undefined): Promise<number> {
  const [row] = await tx.select({ total: sql<number>`count(*)::int` }).from(games).where(where);
  return row?.total ?? 0;
}

function nextDay(day: string): Date {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + 1);
  return d;
}

/** One page of the user's games. Counts cover the player's own moves in finished analyses only. */
export function listGames(db: Db, userId: string, q: GameListQuery): Promise<GamePage> {
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, q.filters.accountId);
    if (ids.length === 0) return { rows: [], total: 0, page: 1, pageCount: 1 };

    const f = q.filters;
    const conds: SQL[] = [inArray(games.accountId, ids)];
    if (f.timeClass) conds.push(eq(games.timeClass, f.timeClass));
    if (f.colour) conds.push(eq(games.playerColor, f.colour));
    if (f.from) conds.push(gte(games.playedAt, new Date(`${f.from}T00:00:00Z`)));
    if (f.to) conds.push(lt(games.playedAt, nextDay(f.to)));
    if (f.result) {
      const wins = sql`((${games.result} = '1-0' and ${games.playerColor} = 'white') or (${games.result} = '0-1' and ${games.playerColor} = 'black'))`;
      if (f.result === "draw") conds.push(eq(games.result, "1/2-1/2"));
      else if (f.result === "win") conds.push(wins);
      else conds.push(sql`(${games.result} in ('1-0','0-1') and not ${wins})`);
    }
    const where = and(...conds);

    // Judgement counts are only needed for the page's rows, except when sorting by them.
    let pageIds: string[] | null = null;
    const total = await countGames(tx, where);
    const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
    const page = Math.min(Math.max(1, q.page), pageCount);
    const offset = (page - 1) * PAGE_SIZE;
    const dir = q.sort.dir === "asc" ? asc : desc;
    const raw = sql.raw(q.sort.dir);
    const accuracy = sql<
      number | null
    >`case when ${games.analysisStatus} = 'done' then case when ${games.playerColor} = 'white' then ${games.whiteAccuracy} else ${games.blackAccuracy} end end`;
    const opponentRating = sql<
      number | null
    >`case when ${games.playerColor} = 'white' then ${games.blackRating} else ${games.whiteRating} end`;
    if (q.sort.key !== "blunders") {
      const first =
        q.sort.key === "accuracy"
          ? sql`${accuracy} ${raw} nulls last`
          : q.sort.key === "opponent"
            ? sql`${opponentRating} ${raw} nulls last`
            : dir(games.playedAt);
      const idRows = await tx
        .select({ id: games.id })
        .from(games)
        .where(where)
        .orderBy(first, desc(games.playedAt), asc(games.id))
        .limit(PAGE_SIZE)
        .offset(offset);
      pageIds = idRows.map((r) => r.id);
      if (pageIds.length === 0) return { rows: [], total, page, pageCount };
    }
    const only = (): SQL | undefined => (pageIds ? inArray(games.id, pageIds) : undefined);

    const mv = tx
      .select({
        gameId: moves.gameId,
        blunders: sql<number>`count(*) filter (where ${moves.judgement} = 'blunder')::int`.as(
          "blunders",
        ),
        mistakes: sql<number>`count(*) filter (where ${moves.judgement} = 'mistake')::int`.as(
          "mistakes",
        ),
        inaccuracies:
          sql<number>`count(*) filter (where ${moves.judgement} = 'inaccuracy')::int`.as(
            "inaccuracies",
          ),
      })
      .from(moves)
      .innerJoin(games, eq(games.id, moves.gameId))
      .where(and(inArray(games.accountId, ids), eq(moves.color, games.playerColor), only()))
      .groupBy(moves.gameId)
      .as("mv");
    const allPlies = tx
      .select({ gameId: moves.gameId, n: sql<number>`count(*)::int`.as("n") })
      .from(moves)
      .innerJoin(games, eq(games.id, moves.gameId))
      .where(and(inArray(games.accountId, ids), only()))
      .groupBy(moves.gameId)
      .as("ap");

    const blunders = sql<
      number | null
    >`case when ${games.analysisStatus} = 'done' then coalesce(${mv.blunders}, 0) end`;
    const order =
      q.sort.key === "accuracy"
        ? sql`${accuracy} ${raw} nulls last`
        : q.sort.key === "blunders"
          ? sql`${blunders} ${raw} nulls last`
          : q.sort.key === "opponent"
            ? sql`${opponentRating} ${raw} nulls last`
            : dir(games.playedAt);
    const rows = await tx
      .select({
        g: games,
        accuracy,
        blunders: mv.blunders,
        mistakes: mv.mistakes,
        inaccuracies: mv.inaccuracies,
        plies: allPlies.n,
      })
      .from(games)
      .leftJoin(mv, eq(mv.gameId, games.id))
      .leftJoin(allPlies, eq(allPlies.gameId, games.id))
      .where(pageIds ? inArray(games.id, pageIds) : where)
      .orderBy(...(pageIds ? [asc(games.id)] : [order, desc(games.playedAt), asc(games.id)]))
      .limit(PAGE_SIZE)
      .offset(pageIds ? 0 : offset);

    if (pageIds) {
      const rank = new Map(pageIds.map((id, i) => [id, i]));
      rows.sort((a, b) => (rank.get(a.g.id) ?? 0) - (rank.get(b.g.id) ?? 0));
    }
    const out: GameRow[] = rows.map(({ g, ...r }) => {
      const white = g.playerColor === "white";
      const done = g.analysisStatus === "done";
      return {
        id: g.id,
        accountId: g.accountId,
        platform: g.platform,
        playedAt: g.playedAt,
        timeClass: g.timeClass as TimeClass,
        baseSeconds: g.baseSeconds,
        incrementSeconds: g.incrementSeconds,
        rated: g.rated,
        playerColor: g.playerColor,
        player: {
          name: white ? g.whiteName : g.blackName,
          rating: white ? g.whiteRating : g.blackRating,
        },
        opponent: {
          name: white ? g.blackName : g.whiteName,
          rating: white ? g.blackRating : g.whiteRating,
        },
        result: playerResult(g.result, g.playerColor),
        termination: g.termination,
        eco: g.eco,
        openingName: g.openingName,
        analysisStatus: g.analysisStatus,
        accuracy: done ? r.accuracy : null,
        counts: done
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
