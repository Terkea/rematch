import { accountLinks, type Db, games, importRuns, linkedAccounts } from "@ci/db";
import { asc, desc, eq, inArray, sql } from "drizzle-orm";
import { byTimeClass, defaultTimeClass } from "../../lib/time-class";
import { type FormatOption, listFormats } from "../filters/formats";
import { countGames, listGames } from "./list";
import { NO_FILTERS, parseGameQuery } from "./query";
import { scopedAccountIds } from "./scope";
import type { GameFilters, GameListQuery, GamePage, TimeClass } from "./types";

type Params = Record<string, string | string[] | undefined>;

/** An account the explorer can filter by: its linked account id and "chess.com name". */
export type ExplorerAccount = { id: string; label: string };

/**
 * A filter the user could drop to see games again, with how many games that would show. `dates`
 * stands for `from` and `to` together.
 */
export type Relax = { key: keyof GameFilters | "dates"; total: number };

/**
 * Where the accounts' imports stand, from each account's latest import run: `importing` while a
 * run is beating (or none has started yet), `failed` when the latest run failed or stopped
 * beating, `done` when it finished. Any importing account makes the whole `importing`.
 */
export type ImportState = "importing" | "failed" | "done";

/** A running import whose heartbeat is older than this is stuck (the reconciler's default). */
export const IMPORT_STALE_MS = 10 * 60_000;

export type Explorer =
  | { state: "no-links" }
  | { state: "no-games"; accounts: ExplorerAccount[]; importState: ImportState }
  | {
      state: "games";
      accounts: ExplorerAccount[];
      query: GameListQuery;
      page: GamePage;
      /** Each time class the user has games in, rapid first (I8), with its game count. */
      classes: ClassCount[];
      /** The format options of the query's time class (the shared bar's Format, Q2). */
      formats: FormatOption[];
      /** Set only when a filtered query matched nothing. */
      relax?: Relax[];
    };

const PLATFORM = { chesscom: "chess.com", lichess: "Lichess" } as const;

export type ClassCount = { tc: TimeClass; games: number };

/** Games per time class over the user's linked accounts, rapid first (I8). */
async function classCounts(db: Db, userId: string): Promise<ClassCount[]> {
  const ids = await scopedAccountIds(db, userId);
  if (ids.length === 0) return [];
  const rows = await db
    .select({ tc: games.timeClass, games: sql<number>`count(*)::int` })
    .from(games)
    .where(inArray(games.accountId, ids))
    .groupBy(games.timeClass);
  return rows.filter((r) => r.games > 0).sort((a, b) => byTimeClass(a.tc, b.tc));
}

/** The user's linked accounts, one per account, with whether each has finished an import. */
async function explorerAccounts(db: Db, userId: string) {
  const rows = await db
    .selectDistinct({
      id: linkedAccounts.id,
      platform: linkedAccounts.platform,
      displayName: linkedAccounts.displayName,
      lastSyncedAt: linkedAccounts.lastSyncedAt,
    })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .where(eq(accountLinks.userId, userId))
    .orderBy(asc(linkedAccounts.platform), asc(linkedAccounts.displayName), asc(linkedAccounts.id));
  return rows.map((r) => ({
    id: r.id,
    label: `${PLATFORM[r.platform]} ${r.displayName}`,
    synced: r.lastSyncedAt !== null,
  }));
}

async function importState(
  db: Db,
  accounts: { id: string; synced: boolean }[],
  now: Date = new Date(),
): Promise<ImportState> {
  const runs = await db
    .selectDistinctOn([importRuns.accountId], {
      accountId: importRuns.accountId,
      status: importRuns.status,
      heartbeatAt: importRuns.heartbeatAt,
    })
    .from(importRuns)
    .where(
      inArray(
        importRuns.accountId,
        accounts.map((a) => a.id),
      ),
    )
    .orderBy(importRuns.accountId, desc(importRuns.startedAt), desc(importRuns.id));
  const states = accounts.map((a): ImportState => {
    const run = runs.find((r) => r.accountId === a.id);
    if (!run) return a.synced ? "done" : "importing";
    if (run.status === "failed") return "failed";
    if (run.status === "running") {
      return now.getTime() - run.heartbeatAt.getTime() > IMPORT_STALE_MS ? "failed" : "importing";
    }
    if (run.status === "paused") return "importing";
    return "done";
  });
  if (states.includes("importing")) return "importing";
  return states.includes("failed") ? "failed" : "done";
}

/** Each set filter (dates as one), with the game count once it is dropped; zero counts left out. */
async function relaxations(db: Db, userId: string, query: GameListQuery): Promise<Relax[]> {
  const f = query.filters;
  const keys: Relax["key"][] = [];
  if (f.timeClass) keys.push("timeClass");
  if (f.format) keys.push("format");
  if (f.result) keys.push("result");
  if (f.colour) keys.push("colour");
  if (f.from || f.to) keys.push("dates");
  if (f.accountId) keys.push("accountId");
  const out: Relax[] = [];
  for (const key of keys) {
    const filters =
      key === "dates"
        ? { ...f, from: null, to: null }
        : key === "timeClass"
          ? { ...f, timeClass: null, format: null }
          : { ...f, [key]: null };
    const total = await countGames(db, userId, filters);
    if (total > 0) out.push({ key, total });
  }
  return out;
}

/**
 * Everything `/app/games` renders: which empty state applies, or the page of games for the URL's
 * query. A filtered query that matches nothing stays in `games` (the "no match" state, with the
 * filters it could drop) as long as the user has any game at all; only an unfiltered empty list is
 * "no games", which says whether an import is still under way.
 */
export async function loadExplorer(db: Db, userId: string, params: Params): Promise<Explorer> {
  const linked = await explorerAccounts(db, userId);
  if (linked.length === 0) return { state: "no-links" };
  const accounts = linked.map(({ id, label }) => ({ id, label }));
  // The first class with games by default, rapid when there is rapid (I8), as on every filtered
  // page (ADR 0083); `tc=all` shows every game.
  const classes = await classCounts(db, userId);
  const parsed = parseGameQuery(params, defaultTimeClass(classes.map((c) => c.tc)));
  const tc = parsed.filters.timeClass;
  const formats = tc ? await listFormats(db, await scopedAccountIds(db, userId), tc) : [];
  // A format the class does not list (another class's, or none played) is dropped, as on every
  // filtered page (ADR 0083).
  const fmt = parsed.filters.format;
  const query =
    fmt !== null && !formats.some((o) => o.key === fmt)
      ? { ...parsed, filters: { ...parsed.filters, format: null } }
      : parsed;
  const page = await listGames(db, userId, query);
  if (page.total > 0) return { state: "games", accounts, query, page, classes, formats };
  const filtered = Object.values(query.filters).some((v) => v !== null);
  const any = filtered ? await countGames(db, userId, NO_FILTERS) : page.total;
  if (any === 0) {
    return { state: "no-games", accounts, importState: await importState(db, linked) };
  }
  return {
    state: "games",
    accounts,
    query,
    page,
    classes,
    formats,
    relax: await relaxations(db, userId, query),
  };
}
