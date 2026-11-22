import type { Db } from "@ci/db";
import type { BetterAuthOptions } from "better-auth";
import { onLichessAccount } from "../accounts/lichess-link";
import { lichessUsername } from "./lichess";

type DbHooks = NonNullable<BetterAuthOptions["databaseHooks"]>;
type AccountRow = { providerId?: unknown; accountId?: unknown; userId?: unknown } | null;

/** A Lichess account row was created (sign-up, link) or refreshed (sign-in, reconnect). */
async function lichessAccountChanged(db: Db, row: AccountRow): Promise<void> {
  if (row?.providerId !== "lichess") return;
  const lichessId = typeof row.accountId === "string" ? row.accountId : null;
  const userId = typeof row.userId === "string" ? row.userId : null;
  if (!lichessId || !userId) return;
  try {
    await onLichessAccount(db, {
      userId,
      lichessId,
      username: lichessUsername(lichessId) ?? lichessId,
    });
  } catch (error) {
    // The OAuth account row is already committed; the link is recorded again on the next sign-in
    // or connect. Logged without the row (it carries the encrypted token).
    console.error("[auth] recording the Lichess link failed", {
      userId,
      lichessId,
      reason: error instanceof Error ? error.message : String(error),
    });
  }
}

/**
 * Better Auth database hooks (ADR-WEB-4). Both run after the transaction commits. The update
 * hook keeps the link verified when a player who is still linked signs in or connects again.
 * Removing a link deletes the OAuth account row too (`removeLink`), so nothing here can restore
 * a removed link. The invite guard is composed in auth.ts.
 */
export function authDbHooks(db: Db): DbHooks {
  return {
    account: {
      create: {
        async after(row) {
          await lichessAccountChanged(db, row as AccountRow);
        },
      },
      update: {
        async after(row) {
          await lichessAccountChanged(db, row as AccountRow);
        },
      },
    },
  };
}
