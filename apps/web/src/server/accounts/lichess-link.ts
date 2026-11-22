import { accountLinks, type Db, linkedAccounts } from "@ci/db";
import { and, eq, isNotNull, ne, sql } from "drizzle-orm";
import { OWNER_LOCK_SPACE } from "./chesscom-link";

export type LichessAccountEvent = {
  userId: string;
  /** Lichess's account id (the lower-case username). */
  lichessId: string;
  /** The username as Lichess shows it. */
  username: string;
};

/**
 * Records a Lichess account proven by OAuth (ADR-WEB-4). Called from the Better Auth account
 * hooks whenever a `lichess` account row is created or refreshed for a user. It upserts the shared
 * `linked_accounts` row and the user's `account_links` row as verified (`lichess_oauth`).
 *
 * The first holder keeps it (review I3): Better Auth already refuses a second user's OAuth link
 * (`account_already_linked_to_different_user`, shown as `lichess_taken`). If another user still
 * holds a verified link anyway, this one is recorded unverified and the other is left alone. The
 * check runs under the per-account owner lock shared with chess.com verification.
 */
export async function onLichessAccount(db: Db, event: LichessAccountEvent): Promise<void> {
  const username = event.lichessId.toLowerCase();
  const displayName = event.username.toLowerCase() === username ? event.username : username;
  const now = new Date();

  await db.transaction(async (tx) => {
    await tx
      .insert(linkedAccounts)
      .values({ platform: "lichess", username, displayName })
      .onConflictDoUpdate({
        target: [linkedAccounts.platform, linkedAccounts.username],
        set: { displayName },
      });
    const [shared] = await tx
      .select({ id: linkedAccounts.id })
      .from(linkedAccounts)
      .where(and(eq(linkedAccounts.platform, "lichess"), eq(linkedAccounts.username, username)));
    if (!shared) throw new Error("onLichessAccount: linked account vanished");

    await tx.execute(
      sql`select pg_advisory_xact_lock(${OWNER_LOCK_SPACE}, hashtext(${shared.id}))`,
    );
    const heldElsewhere = await tx
      .select({ id: accountLinks.id })
      .from(accountLinks)
      .where(
        and(
          eq(accountLinks.linkedAccountId, shared.id),
          ne(accountLinks.userId, event.userId),
          isNotNull(accountLinks.verifiedAt),
        ),
      );
    const verifiedAt = heldElsewhere.length > 0 ? null : now;
    await tx
      .insert(accountLinks)
      .values({
        userId: event.userId,
        linkedAccountId: shared.id,
        method: "lichess_oauth",
        verifiedAt,
      })
      .onConflictDoUpdate({
        target: [accountLinks.userId, accountLinks.linkedAccountId],
        set: { method: "lichess_oauth", verifiedAt },
      });
  });
}
