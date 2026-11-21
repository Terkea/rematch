import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";

/**
 * Outbound chess.com throttles (ADR-WEB-5). Every profile request made on a user's behalf (start
 * and verify alike, unknown usernames included) needs both:
 * - a token from the user's bucket: 10 per hour, refilled continuously;
 * - the username's slot: at most one request per username per 60 s, across all users.
 * Both live in their own tables, keyed by user and by username, so deleting and re-adding a link
 * resets neither. The username is checked first, so a blocked request spends no token.
 */
export const USER_BUCKET_CAPACITY = 10;
export const USER_BUCKET_REFILL_MS = 3600_000 / USER_BUCKET_CAPACITY;
export const USERNAME_INTERVAL_MS = 60_000;
const PRUNE_AFTER_MS = 3600_000;

export type Claim =
  | { ok: true }
  | { ok: false; reason: "username_interval" | "user_budget"; retryAt: Date };

const ts = (date: Date) => sql`${date.toISOString()}::timestamptz`;

/** Claims one profile request for `userId` on `username` (lower-case) at `now`, atomically. */
export async function claimProfileRequest(
  db: Db,
  userId: string,
  username: string,
  now: Date,
): Promise<Claim> {
  return db.transaction(async (tx) => {
    await tx.execute(sql`
      insert into chesscom_profile_requests (username, requested_at)
      values (${username}, 'epoch'::timestamptz) on conflict do nothing`);
    const [slot] = await tx.execute<{ ms: number }>(sql`
      select (extract(epoch from requested_at) * 1000)::float8 as ms from chesscom_profile_requests
      where username = ${username} for update`);
    const last = new Date(Number(slot?.ms ?? 0));
    const free = new Date(last.getTime() + USERNAME_INTERVAL_MS);
    if (free > now) return { ok: false, reason: "username_interval", retryAt: free };

    await tx.execute(sql`
      insert into chesscom_request_budgets (user_id, tokens, refilled_at)
      values (${userId}, ${USER_BUCKET_CAPACITY}, ${ts(now)}) on conflict do nothing`);
    const [budget] = await tx.execute<{ tokens: number; ms: number }>(sql`
      select tokens, (extract(epoch from refilled_at) * 1000)::float8 as ms
      from chesscom_request_budgets
      where user_id = ${userId} for update`);
    const elapsed = Math.max(0, now.getTime() - Number(budget?.ms ?? now.getTime()));
    const tokens = Math.min(
      USER_BUCKET_CAPACITY,
      Number(budget?.tokens ?? USER_BUCKET_CAPACITY) + elapsed / USER_BUCKET_REFILL_MS,
    );
    if (tokens < 1) {
      // Leave no placeholder behind for a name that was never requested.
      await tx.execute(sql`
        delete from chesscom_profile_requests
        where username = ${username} and requested_at = 'epoch'::timestamptz`);
      const retryAt = new Date(now.getTime() + Math.ceil((1 - tokens) * USER_BUCKET_REFILL_MS));
      return { ok: false, reason: "user_budget", retryAt };
    }

    await tx.execute(sql`
      update chesscom_request_budgets set tokens = ${tokens - 1}, refilled_at = ${ts(now)}
      where user_id = ${userId}`);
    await tx.execute(sql`
      update chesscom_profile_requests set requested_at = ${ts(now)} where username = ${username}`);
    await tx.execute(sql`
      delete from chesscom_profile_requests
      where requested_at < ${ts(new Date(now.getTime() - PRUNE_AFTER_MS))}`);
    return { ok: true };
  });
}
