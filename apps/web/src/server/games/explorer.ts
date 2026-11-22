import { accountLinks, type Db, importRuns, linkedAccounts } from "@ci/db";
import { asc, desc, eq, inArray } from "drizzle-orm";
import { countGames, listGames } from "./list";
import { NO_FILTERS, parseGameQuery } from "./query";
import type { GameFilters, GameListQuery, GamePage } from "./types";

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
      /** Set only when a filtered query matched nothing. */
      relax?: Relax[];
    };

const PLATFORM = { chesscom: "chess.com", lichess: "Lichess" } as const;

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
  if (f.result) keys.push("result");
  if (f.colour) keys.push("colour");
  if (f.from || f.to) keys.push("dates");
  if (f.accountId) keys.push("accountId");
  const out: Relax[] = [];
  for (const key of keys) {
    const filters = key === "dates" ? { ...f, from: null, to: null } : { ...f, [key]: null };
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
  const query = parseGameQuery(params);
  const page = await listGames(db, userId, query);
  if (page.total > 0) return { state: "games", accounts, query, page };
  const filtered = Object.values(query.filters).some((v) => v !== null);
  const any = filtered ? await countGames(db, userId, NO_FILTERS) : page.total;
  if (any === 0) {
    return { state: "no-games", accounts, importState: await importState(db, linked) };
  }
  return { state: "games", accounts, query, page, relax: await relaxations(db, userId, query) };
}
