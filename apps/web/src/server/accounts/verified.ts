import type { Db } from "@ci/db";
import { accountLinks } from "@ci/db/generated";
import { and, eq, isNotNull, sql } from "drizzle-orm";
import { getEnv } from "../../env";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * Whether the user's own link to the account is verified (`account_links.verified_at`). The
 * owner's ruling on holistic F10, "analysis only": anyone may link a public username and see the
 * computed analysis of its games, but the account's AI artefacts (the report, the change notes,
 * the weekly reviews, the coach and its allowance) are read only through a verified link.
 */
export async function isVerifiedLink(db: Db, userId: string, accountId: string): Promise<boolean> {
  if (!UUID.test(accountId)) return false;
  // Local only (ADR-FIX-N-1 Amendment 1): any link of the user's own counts as verified. The
  // flag is refused in production (env.ts), and the user must still have a link to the account.
  const anyLink = getEnv().ALLOW_UNVERIFIED_LINKS;
  const rows = await db
    .select({ one: sql<number>`1` })
    .from(accountLinks)
    .where(
      and(
        eq(accountLinks.userId, userId),
        eq(accountLinks.linkedAccountId, accountId),
        anyLink ? undefined : isNotNull(accountLinks.verifiedAt),
      ),
    )
    .limit(1);
  return rows.length > 0;
}
