import { accountLinks, type Db } from "@ci/db";
import { and, eq } from "drizzle-orm";
import { LinkError } from "./chesscom-link";

/**
 * Removes one of the user's own links (and, by cascade, its verification code). The shared
 * `linked_accounts` row and its games stay: other users may link the same account.
 * Throws `LinkError("link_not_found")` when the link is missing or someone else's.
 */
export async function removeLink(db: Db, userId: string, linkId: string): Promise<void> {
  const removed = await db
    .delete(accountLinks)
    .where(and(eq(accountLinks.id, linkId), eq(accountLinks.userId, userId)))
    .returning({ id: accountLinks.id });
  if (removed.length === 0) throw new LinkError("link_not_found");
}
