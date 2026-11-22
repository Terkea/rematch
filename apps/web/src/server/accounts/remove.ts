import { account, accountLinks, type Db, linkedAccounts, user } from "@ci/db";
import { and, eq, ne } from "drizzle-orm";
import { isLichessPlaceholderEmail } from "../auth/lichess";
import { LinkError } from "./chesscom-link";

export type RemoveLinkOptions = {
  /**
   * Called after commit with the stored (encrypted) Lichess access token of a removed Lichess link,
   * to revoke it at Lichess. Best effort; it must never log the token.
   */
  revokeLichessToken?: (storedToken: string) => Promise<void> | void;
};

/**
 * Removes one of the user's own links (and, by cascade, its verification code). The shared
 * `linked_accounts` row and its games stay: other users may link the same account.
 *
 * A Lichess link also loses its OAuth `account` row and stored token in the same transaction, so a
 * later "Continue with Lichess" starts afresh instead of restoring the link (ADR-WEB-4, review I2).
 * It is refused (`last_sign_in_method`) when Lichess is the only way into the account: a
 * Lichess-created account has a placeholder email and no other provider.
 *
 * Throws `LinkError("link_not_found")` when the link is missing or someone else's.
 */
export async function removeLink(
  db: Db,
  userId: string,
  linkId: string,
  options: RemoveLinkOptions = {},
): Promise<void> {
  const revoked = await db.transaction(async (tx) => {
    const [row] = await tx
      .select({ platform: linkedAccounts.platform, username: linkedAccounts.username })
      .from(accountLinks)
      .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
      .where(and(eq(accountLinks.id, linkId), eq(accountLinks.userId, userId)))
      .for("update", { of: accountLinks });
    if (!row) throw new LinkError("link_not_found");

    let token: string | null = null;
    if (row.platform === "lichess") {
      const [owner] = await tx.select({ email: user.email }).from(user).where(eq(user.id, userId));
      const others = await tx
        .select({ id: account.id })
        .from(account)
        .where(and(eq(account.userId, userId), ne(account.providerId, "lichess")));
      if (owner && isLichessPlaceholderEmail(owner.email) && others.length === 0) {
        throw new LinkError("last_sign_in_method");
      }
      const deleted = await tx
        .delete(account)
        .where(
          and(
            eq(account.userId, userId),
            eq(account.providerId, "lichess"),
            eq(account.accountId, row.username),
          ),
        )
        .returning({ accessToken: account.accessToken });
      token = deleted[0]?.accessToken ?? null;
    }

    await tx.delete(accountLinks).where(eq(accountLinks.id, linkId));
    return token;
  });
  if (revoked && options.revokeLichessToken) await options.revokeLichessToken(revoked);
}
