import type { Db } from "@ci/db";
import { userAccountIds } from "../events/user-accounts";

/**
 * The account ids whose games the user may read: every link they hold, verified or not (ruling
 * R1, ADR 0046), narrowed to `only` when it is one of them, or `[]` when `only` is foreign.
 */
export async function scopedAccountIds(
  db: Db,
  userId: string,
  only?: string | null,
): Promise<string[]> {
  const ids = await userAccountIds(db, userId);
  if (only == null) return ids;
  return ids.includes(only) ? [only] : [];
}
