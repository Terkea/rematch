import { accountLinks, type Db } from "@ci/db";
import { asc, eq } from "drizzle-orm";

/**
 * The linked account ids whose events this user receives: every one of their `accountLinks`,
 * verified or not (ADR-WEB-8). Read when a stream opens; a link added later is picked up on the
 * client's next reconnect.
 */
export async function userAccountIds(db: Db, userId: string): Promise<string[]> {
  const rows = await db
    .selectDistinct({ id: accountLinks.linkedAccountId })
    .from(accountLinks)
    .where(eq(accountLinks.userId, userId))
    .orderBy(asc(accountLinks.linkedAccountId));
  return rows.map((row) => row.id);
}
