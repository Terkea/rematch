import {
  accountLinks,
  account as authAccount,
  chesscomVerifications,
  type Db,
  linkedAccounts,
} from "@ci/db";
import { and, asc, eq } from "drizzle-orm";
import { lichessLinkStatus } from "../auth/lichess";

export type AccountStatus = "ok" | "expiring" | "expired" | "pending_verification";

export type LinkedAccountView = {
  linkId: string;
  platform: "lichess" | "chesscom";
  username: string;
  displayName: string;
  verified: boolean;
  /**
   * `ok`: verified. `pending_verification`: a chess.com code is waiting in `code`. `expired`: the
   * code ran out (start the link again for a fresh one). For Lichess, `expiring` and `expired`
   * follow the OAuth token's expiry (`lichessLinkStatus`), and both offer "Reconnect Lichess".
   */
  status: AccountStatus;
  code?: string;
};

/** The user's own links, oldest first. Codes are shown only while they can still be used. */
export async function listAccounts(
  db: Db,
  userId: string,
  now: Date = new Date(),
): Promise<LinkedAccountView[]> {
  const rows = await db
    .select({
      link: accountLinks,
      account: linkedAccounts,
      verification: chesscomVerifications,
      // Only the expiry: tokens never leave the database through this query.
      tokenExpiresAt: authAccount.accessTokenExpiresAt,
      oauthAccountId: authAccount.id,
    })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .leftJoin(chesscomVerifications, eq(chesscomVerifications.accountLinkId, accountLinks.id))
    .leftJoin(
      authAccount,
      and(
        eq(linkedAccounts.platform, "lichess"),
        eq(authAccount.providerId, "lichess"),
        eq(authAccount.userId, accountLinks.userId),
        eq(authAccount.accountId, linkedAccounts.username),
      ),
    )
    .where(eq(accountLinks.userId, userId))
    .orderBy(asc(accountLinks.createdAt), asc(accountLinks.id));

  return rows.map(({ link, account, verification, tokenExpiresAt, oauthAccountId }) => {
    const base = {
      linkId: link.id,
      platform: account.platform,
      username: account.username,
      displayName: account.displayName,
      verified: link.verifiedAt !== null,
    };
    if (account.platform === "lichess") {
      // Without its OAuth account (or once displaced by a newer owner) the link needs a reconnect.
      if (!link.verifiedAt || !oauthAccountId) return { ...base, status: "expired" as const };
      return { ...base, status: lichessLinkStatus({ accessTokenExpiresAt: tokenExpiresAt }, now) };
    }
    if (link.verifiedAt) return { ...base, status: "ok" as const };
    if (verification && verification.expiresAt > now) {
      return { ...base, status: "pending_verification" as const, code: verification.code };
    }
    return { ...base, status: "expired" as const };
  });
}
