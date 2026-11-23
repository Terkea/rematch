import { accountLinks, type Db, games, importRuns, linkedAccounts, moves } from "@ci/db";
import { and, desc, eq, inArray, sql } from "drizzle-orm";
import { TIME_CLASS_ORDER } from "../../lib/time-class";
import { listGames, playerResult, readOnly, scopedAccountIds } from "../games";
import type { GameRow, PlayerResult, TimeClass } from "../games/types";
import {
  CHART_GAMES,
  type Estimate,
  estimateMean,
  estimateRate,
  PER_GAME_COLUMNS,
  rollingMean,
  TREND_WEEKS,
} from "../stats";
import type { Overview } from "./types";

const DAY = 86_400_000;
const PERCENT = { min: 0, max: 100 };

type GameStat = {
  id: string;
  playedAt: Date;
  timeClass: TimeClass;
  result: PlayerResult;
  accuracy: number | null;
  counts: { blunders: number; mistakes: number; inaccuracies: number } | null;
};

/** ISO week key ("2026-W24") of a UTC instant, and the Thursday that anchors it. */
function isoWeek(d: Date): { key: string; thursday: number } {
  const midnight = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
  const dayNum = (new Date(midnight).getUTCDay() + 6) % 7;
  const thursday = midnight - dayNum * DAY + 3 * DAY;
  const year = new Date(thursday).getUTCFullYear();
  const week = Math.ceil(((thursday - Date.UTC(year, 0, 1)) / DAY + 1) / 7);
  return { key: `${year}-W${String(week).padStart(2, "0")}`, thursday };
}

const seedOf = (metric: string, ids: string[]) => `${metric}:${[...ids].sort().join(",")}`;

function mean(values: number[]): number | null {
  const clean = values.filter(Number.isFinite);
  return clean.length === 0 ? null : clean.reduce((a, b) => a + b, 0) / clean.length;
}

function emptyOverview(accounts: Overview["accounts"]): Overview {
  const none = (): Estimate => estimateMean([], "none");
  return {
    accounts,
    status: { total: 0, analysed: 0, waiting: 0, failed: 0, skipped: 0 },
    timeClasses: [],
    accuracy: { points: [], rolling: [], overall: none() },
    judgements: {
      columns: [],
      blunders: none(),
      mistakes: none(),
      inaccuracies: none(),
      weekly: [],
    },
    byTimeClass: [],
    recent: [],
  };
}

export async function getOverview(
  db: Db,
  userId: string,
  opts: { timeClass: TimeClass | null },
): Promise<Overview> {
  const data = await readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId);
    if (ids.length === 0) return null;

    const links = await tx
      .select({
        id: linkedAccounts.id,
        platform: linkedAccounts.platform,
        username: linkedAccounts.username,
        displayName: linkedAccounts.displayName,
        verifiedAt: accountLinks.verifiedAt,
        lastSyncedAt: linkedAccounts.lastSyncedAt,
      })
      .from(accountLinks)
      .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
      .where(eq(accountLinks.userId, userId))
      .orderBy(linkedAccounts.createdAt);
    const runs = await tx
      .selectDistinctOn([importRuns.accountId])
      .from(importRuns)
      .where(inArray(importRuns.accountId, ids))
      .orderBy(importRuns.accountId, desc(importRuns.startedAt));

    const statusRows = await tx
      .select({ status: games.analysisStatus, n: sql<number>`count(*)::int` })
      .from(games)
      .where(inArray(games.accountId, ids))
      .groupBy(games.analysisStatus);

    const isWin = sql`((${games.result} = '1-0' and ${games.playerColor} = 'white') or (${games.result} = '0-1' and ${games.playerColor} = 'black'))`;
    const isDecided = sql`${games.result} in ('1-0', '0-1')`;
    const classRows = await tx
      .select({
        timeClass: games.timeClass,
        analysed: sql<number>`count(*)::int`,
        wins: sql<number>`(count(*) filter (where ${isWin}))::int`,
        draws: sql<number>`(count(*) filter (where ${games.result} = '1/2-1/2'))::int`,
        losses: sql<number>`(count(*) filter (where ${isDecided} and not ${isWin}))::int`,
      })
      .from(games)
      .where(and(inArray(games.accountId, ids), eq(games.analysisStatus, "done")))
      .groupBy(games.timeClass);

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
      .where(
        and(
          inArray(games.accountId, ids),
          eq(games.analysisStatus, "done"),
          eq(moves.color, games.playerColor),
        ),
      )
      .groupBy(moves.gameId)
      .as("mv");
    const perGame = await tx
      .select({
        id: games.id,
        playedAt: games.playedAt,
        timeClass: games.timeClass,
        result: games.result,
        colour: games.playerColor,
        accuracy: sql<
          number | null
        >`case when ${games.playerColor} = 'white' then ${games.whiteAccuracy} else ${games.blackAccuracy} end`,
        blunders: mv.blunders,
        mistakes: mv.mistakes,
        inaccuracies: mv.inaccuracies,
      })
      .from(games)
      .leftJoin(mv, eq(mv.gameId, games.id))
      .where(and(inArray(games.accountId, ids), eq(games.analysisStatus, "done")))
      .orderBy(games.playedAt, games.id);

    return { links, runs, statusRows, classRows, perGame };
  });

  if (!data) return emptyOverview([]);

  const accounts: Overview["accounts"] = data.links.map((l) => {
    const run = data.runs.find((r) => r.accountId === l.id);
    return {
      id: l.id,
      platform: l.platform,
      username: l.username,
      displayName: l.displayName,
      verified: l.verifiedAt !== null,
      lastSyncedAt: l.lastSyncedAt,
      lastImport: run
        ? { status: run.status, imported: run.imported, finishedAt: run.finishedAt }
        : null,
    };
  });

  const count = (s: string) => data.statusRows.find((r) => r.status === s)?.n ?? 0;
  const total = data.statusRows.reduce((a, r) => a + r.n, 0);
  const status = {
    total,
    analysed: count("done"),
    waiting: count("pending") + count("analysing"),
    failed: count("failed"),
    skipped: count("skipped"),
  };

  const classes = TIME_CLASS_ORDER.flatMap((tc) => {
    const row = data.classRows.find((r) => r.timeClass === tc);
    return row && row.analysed > 0 ? [{ tc, row }] : [];
  });
  const stats: GameStat[] = data.perGame.map((g) => ({
    id: g.id,
    playedAt: g.playedAt,
    timeClass: g.timeClass as TimeClass,
    result: playerResult(g.result, g.colour),
    accuracy: g.accuracy !== null && Number.isFinite(g.accuracy) ? g.accuracy : null,
    counts:
      g.blunders === null
        ? null
        : { blunders: g.blunders, mistakes: g.mistakes ?? 0, inaccuracies: g.inaccuracies ?? 0 },
  }));

  const byTimeClass: Overview["byTimeClass"] = classes.map(({ tc, row }) => {
    const accs = stats.filter((s) => s.timeClass === tc && s.accuracy !== null);
    return {
      timeClass: tc,
      games: row.analysed,
      wins: row.wins,
      draws: row.draws,
      losses: row.losses,
      score: estimateRate(row.wins + row.draws / 2, row.wins + row.draws + row.losses),
      accuracy: estimateMean(
        accs.map((s) => s.accuracy as number),
        seedOf(
          `accuracy:${tc}`,
          accs.map((s) => s.id),
        ),
        PERCENT,
      ),
    };
  });

  const scoped = opts.timeClass ? stats.filter((s) => s.timeClass === opts.timeClass) : stats;
  const withAcc = scoped.filter((s) => s.accuracy !== null);
  const allAccIds = withAcc.map((s) => s.id);
  const points = withAcc.slice(-CHART_GAMES).map((s) => ({
    id: s.id,
    playedAt: s.playedAt,
    accuracy: s.accuracy as number,
    result: s.result,
  }));
  const rolling = rollingMean(
    points.map((p) => ({ id: p.id, value: p.accuracy })),
    seedOf(
      "rolling",
      points.map((p) => p.id),
    ),
  );

  const withCounts = scoped.filter((s) => s.counts !== null);
  const countIds = withCounts.map((s) => s.id);
  const metric = (k: "blunders" | "mistakes" | "inaccuracies") =>
    estimateMean(
      withCounts.map((s) => s.counts?.[k] ?? 0),
      seedOf(k, countIds),
    );
  const columns = withCounts.slice(-PER_GAME_COLUMNS).map((s) => ({
    id: s.id,
    playedAt: s.playedAt,
    blunders: s.counts?.blunders ?? 0,
    mistakes: s.counts?.mistakes ?? 0,
    inaccuracies: s.counts?.inaccuracies ?? 0,
  }));

  // Weekly trend (R4): the 12 ISO weeks ending in the latest analysed game's week.
  const weekly: Overview["judgements"]["weekly"] = [];
  const latest = scoped.at(-1);
  if (latest) {
    const end = isoWeek(latest.playedAt).thursday;
    const byWeek = new Map<string, GameStat[]>();
    for (const s of scoped) {
      const k = isoWeek(s.playedAt).key;
      const bucket = byWeek.get(k);
      if (bucket) bucket.push(s);
      else byWeek.set(k, [s]);
    }
    for (let w = TREND_WEEKS - 1; w >= 0; w--) {
      const key = isoWeek(new Date(end - w * 7 * DAY)).key;
      const g = byWeek.get(key) ?? [];
      const c = g.filter((s) => s.counts !== null);
      weekly.push({
        week: key,
        blunders: mean(c.map((s) => s.counts?.blunders ?? 0)),
        mistakes: mean(c.map((s) => s.counts?.mistakes ?? 0)),
        inaccuracies: mean(c.map((s) => s.counts?.inaccuracies ?? 0)),
        accuracy: mean(g.flatMap((s) => (s.accuracy === null ? [] : [s.accuracy]))),
        games: g.length,
      });
    }
  }

  const page = await listGames(db, userId, {
    filters: {
      timeClass: opts.timeClass,
      format: null,
      result: null,
      colour: null,
      from: null,
      to: null,
      accountId: null,
    },
    sort: { key: "played", dir: "desc" },
    page: 1,
  });
  const recent: GameRow[] = page.rows.slice(0, 6);

  return {
    accounts,
    status,
    timeClasses: classes.map(({ tc, row }) => ({ timeClass: tc, analysed: row.analysed })),
    accuracy: {
      points,
      rolling,
      overall: estimateMean(
        withAcc.map((s) => s.accuracy as number),
        seedOf("accuracy", allAccIds),
        PERCENT,
      ),
    },
    judgements: {
      columns,
      blunders: metric("blunders"),
      mistakes: metric("mistakes"),
      inaccuracies: metric("inaccuracies"),
      weekly,
    },
    byTimeClass,
    recent,
  };
}
