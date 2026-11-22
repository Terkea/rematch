import { type Db, invites } from "@ci/db";
import { eq } from "drizzle-orm";
import type { WebEnv } from "../../env";

/** Error code for an address that is not on the allow-list. It says nothing about existing accounts. */
export const NOT_INVITED = "not_invited";

export type InviteSettings = Pick<WebEnv, "INVITE_ONLY" | "INVITE_ALLOWLIST">;

/**
 * The canonical form of an address: NFKC, trimmed, lower-cased. Returns null for anything outside
 * printable ASCII after NFKC, so look-alike or compatibility characters can never match an invite
 * (no internationalised addresses in the beta).
 */
export function normaliseEmail(email: string): string | null {
  const normalised = email.normalize("NFKC").trim().toLowerCase();
  if (!normalised || !/^[\x21-\x7e]+$/.test(normalised)) return null;
  return normalised;
}

/**
 * Private beta gate (ADR 0041, ADR-WEB-3). With `INVITE_ONLY` off every address passes; with it on
 * (the default) an address passes when it is in `INVITE_ALLOWLIST` or in the `invites` table.
 * Settings come from the environment, so changing them needs a restart.
 */
export async function isInvited(db: Db, settings: InviteSettings, email: string): Promise<boolean> {
  if (!settings.INVITE_ONLY) return true;
  const normalised = normaliseEmail(email);
  if (!normalised) return false;
  if (settings.INVITE_ALLOWLIST.includes(normalised)) return true;
  const rows = await db
    .select({ id: invites.id })
    .from(invites)
    .where(eq(invites.email, normalised))
    .limit(1);
  return rows.length > 0;
}
