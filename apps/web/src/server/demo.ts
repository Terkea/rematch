import { accountLinks, type Db, linkedAccounts } from "@ci/db";
import { and, eq, sql } from "drizzle-orm";

/**
 * The fictional demo player that `pnpm seed:demo` writes (ADR 0097) is recognised by its
 * username: neither chess.com nor Lichess allows a dot in one, so no real account can carry the
 * prefix. Rust's sync uses the same prefix (`analysis::jobs::sync::DEMO_USERNAME_PREFIX`).
 */
export const DEMO_USERNAME_PREFIX = "demo.";

export function isDemoUsername(username: string): boolean {
  return username.startsWith(DEMO_USERNAME_PREFIX);
}

const demoLike = `${DEMO_USERNAME_PREFIX.replaceAll("_", "\\_").replaceAll("%", "\\%")}%`;

/** True when one of the user's linked accounts is the demo player (the header shows "Demo data"). */
export async function hasDemoAccount(db: Db, userId: string): Promise<boolean> {
  const [row] = await db
    .select({ id: linkedAccounts.id })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .where(and(eq(accountLinks.userId, userId), sql`${linkedAccounts.username} like ${demoLike}`))
    .limit(1);
  return row !== undefined;
}

/** Of `ids`, the demo player's accounts (never synced from a platform). */
export async function demoAccountIds(db: Db, ids: string[]): Promise<Set<string>> {
  if (ids.length === 0) return new Set();
  const rows = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(
      and(
        sql`${linkedAccounts.id} in (${sql.join(
          ids.map((id) => sql`${id}::uuid`),
          sql`, `,
        )})`,
        sql`${linkedAccounts.username} like ${demoLike}`,
      ),
    );
  return new Set(rows.map((r) => r.id));
}
