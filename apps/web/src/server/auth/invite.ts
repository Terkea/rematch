import { type Db, invites } from "@ci/db";
import { eq } from "drizzle-orm";
import type { WebEnv } from "../../env";

/** Error code for an address that is not on the allow-list. It says nothing about existing accounts. */
export const NOT_INVITED = "not_invited";

export type InviteSettings = Pick<WebEnv, "INVITE_ONLY" | "INVITE_ALLOWLIST">;

/**
 * Private beta gate (ADR 0041, ADR-WEB-3). With `INVITE_ONLY` off every address passes; with it on
 * (the default) an address passes when it is in `INVITE_ALLOWLIST` or in the `invites` table.
 */
export async function isInvited(db: Db, settings: InviteSettings, email: string): Promise<boolean> {
  if (!settings.INVITE_ONLY) return true;
  const normalised = email.trim().toLowerCase();
  if (!normalised) return false;
  if (settings.INVITE_ALLOWLIST.includes(normalised)) return true;
  const rows = await db
    .select({ id: invites.id })
    .from(invites)
    .where(eq(invites.email, normalised))
    .limit(1);
  return rows.length > 0;
}
