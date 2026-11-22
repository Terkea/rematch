import type { Db } from "@ci/db";
import { gameMetrics, linkedAccounts } from "@ci/db/generated";
import { eq, inArray, sql } from "drizzle-orm";
import { byTimeClass, TIME_CLASS_ORDER as TIME_CLASSES } from "../../lib/time-class";
import { readOnly, scopedAccountIds } from "../games";
import type { TimeClass } from "../games/types";
import type { CoachScope } from "./types";

const PLATFORMS = ["lichess", "chesscom"] as const;

type Params = Record<string, string | string[] | undefined>;

function first(v: string | string[] | undefined): string | null {
  const s = Array.isArray(v) ? v[0] : v;
  return s ? s : null;
}

/**
 * True when the scope's account is one of the user's links (M5a R1). Every profile read checks
 * it inside its transaction, so a scope built for another user reads nothing.
 */
export async function ownsScope(tx: Db, userId: string, scope: CoachScope): Promise<boolean> {
  return (await scopedAccountIds(tx, userId, scope.accountId)).length > 0;
}

/**
 * Every (account, time class) of the user with analysed games: rapid first, then blitz, then the
 * rest (I8), and within a time class the most analysed games first (ruling R1).
 */
async function scopeOptions(tx: Db, userId: string): Promise<CoachScope["options"]> {
  const ids = await scopedAccountIds(tx, userId);
  if (ids.length === 0) return [];
  const rows = await tx
    .select({
      accountId: gameMetrics.accountId,
      timeClass: gameMetrics.timeClass,
      platform: linkedAccounts.platform,
      username: linkedAccounts.username,
      analysedGames: sql<number>`count(*)::int`,
    })
    .from(gameMetrics)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, gameMetrics.accountId))
    .where(inArray(gameMetrics.accountId, ids))
    .groupBy(
      gameMetrics.accountId,
      gameMetrics.timeClass,
      linkedAccounts.platform,
      linkedAccounts.username,
    );
  return rows.sort(
    (a, b) =>
      byTimeClass(a.timeClass, b.timeClass) ||
      b.analysedGames - a.analysedGames ||
      PLATFORMS.indexOf(a.platform) - PLATFORMS.indexOf(b.platform) ||
      a.username.localeCompare(b.username) ||
      (a.accountId < b.accountId ? -1 : a.accountId > b.accountId ? 1 : 0),
  );
}

/**
 * The coach scope (ruling R1, ADR 0068): the `account` and `tc` params narrow the user's options
 * when they name one; a foreign, malformed or empty param is ignored, and a choice that matches
 * no option falls back to the default, the first option: rapid when there is rapid (I8), with
 * the most `game_metrics` rows. Null when
 * the user has no analysed game.
 */
export async function resolveCoachScope(
  db: Db,
  userId: string,
  params: Params,
): Promise<CoachScope | null> {
  return readOnly(db, (tx) => resolveIn(tx, userId, params));
}

export async function resolveIn(
  tx: Db,
  userId: string,
  params: Params,
): Promise<CoachScope | null> {
  const options = await scopeOptions(tx, userId);
  const fallback = options[0];
  if (!fallback) return null;
  const rawAccount = first(params.account);
  const rawTc = first(params.tc);
  const account = options.some((o) => o.accountId === rawAccount) ? rawAccount : null;
  const tc = TIME_CLASSES.includes(rawTc as TimeClass) ? rawTc : null;
  const chosen =
    options.find(
      (o) => (account == null || o.accountId === account) && (tc == null || o.timeClass === tc),
    ) ?? fallback;
  return {
    accountId: chosen.accountId,
    timeClass: chosen.timeClass,
    platform: chosen.platform,
    username: chosen.username,
    options,
  };
}
