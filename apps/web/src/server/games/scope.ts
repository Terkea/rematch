import { accountLinks, type Db } from "@ci/db";
import { eq } from "drizzle-orm";

/**
 * The account ids the user holds a link to, verified or not (ruling R1), narrowed to `only` when
 * it is one of them, or `[]` when `only` is foreign.
 * (Reads `account_links` directly until M4 T22's `userAccountIds` lands; Task 1 swaps it in.)
 */
export async function scopedAccountIds(
  db: Db,
  userId: string,
  only?: string | null,
): Promise<string[]> {
  const rows = await db
    .select({ id: accountLinks.linkedAccountId })
    .from(accountLinks)
    .where(eq(accountLinks.userId, userId));
  const ids = rows.map((r) => r.id);
  if (only == null) return ids;
  return ids.includes(only) ? [only] : [];
}
