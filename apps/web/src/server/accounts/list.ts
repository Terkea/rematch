import { accountLinks, chesscomVerifications, type Db, linkedAccounts } from "@ci/db";
import { asc, eq } from "drizzle-orm";

export type AccountStatus = "ok" | "expiring" | "expired" | "pending_verification";

export type LinkedAccountView = {
  linkId: string;
  platform: "lichess" | "chesscom";
  username: string;
  displayName: string;
  verified: boolean;
  /**
   * `ok`: verified. `pending_verification`: a chess.com code is waiting in `code`. `expired`: the
   * code ran out (start the link again for a fresh one). `expiring` is for OAuth links whose token
   * is about to lapse (Lichess, Task 18).
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
    .select({ link: accountLinks, account: linkedAccounts, verification: chesscomVerifications })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .leftJoin(chesscomVerifications, eq(chesscomVerifications.accountLinkId, accountLinks.id))
    .where(eq(accountLinks.userId, userId))
    .orderBy(asc(accountLinks.createdAt), asc(accountLinks.id));

  return rows.map(({ link, account, verification }) => {
    const base = {
      linkId: link.id,
      platform: account.platform,
      username: account.username,
      displayName: account.displayName,
      verified: link.verifiedAt !== null,
    };
    if (link.verifiedAt) return { ...base, status: "ok" as const };
    if (verification && verification.expiresAt > now) {
      return { ...base, status: "pending_verification" as const, code: verification.code };
    }
    return { ...base, status: "expired" as const };
  });
}
