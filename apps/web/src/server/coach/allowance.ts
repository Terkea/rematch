import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { isVerifiedLink } from "../accounts/verified.js";
import { readOnly } from "../games/read-only.js";
import type { QuotaWindow } from "./copy.js";
import { isUuid } from "./threads.js";

// The chat allowance the composer shows (R6, ADR 0070), counted exactly like the `coach-chat`
// job's `used_per_window` (backend/crates/ai/src/chat/job.rs): assistant answers in the account's
// conversations that are `streaming`, or `done` with a non-null `ai_disclosure` (fixed no-data
// copy is free), created inside each UTC window. The web never refuses a post for quota: the job
// is the authority and its denial arrives as `chat.message.failed`.

/**
 * `free` and `pro` are Rust `entitlements.rs`; `beta` is ruling G1 (100 a week, configurable).
 * M7 replaces `aiPlan()` with the billing-backed entitlements.
 */
export type ChatPlan = "free" | "beta" | "pro";

export type Quota = { window: QuotaWindow; limit: number };

export const BETA_CHAT_WEEKLY_DEFAULT = 100;

function betaWeekly(): number {
  const raw = process.env.AI_BETA_CHAT_WEEKLY;
  const n = raw === undefined || raw.trim() === "" ? Number.NaN : Number(raw);
  return Number.isInteger(n) && n > 0 ? n : BETA_CHAT_WEEKLY_DEFAULT;
}

/** The plan's chat quotas, in Rust's order (the first exhausted one is reported). */
export function chatQuotas(plan: ChatPlan): Quota[] {
  switch (plan) {
    case "free":
      return [{ window: "week", limit: 3 }];
    case "beta":
      return [{ window: "week", limit: betaWeekly() }];
    case "pro":
      return [
        { window: "day", limit: 30 },
        { window: "month", limit: 300 },
      ];
  }
}

/** The deployment's plan (`AI_DEFAULT_PLAN`, the worker's setting; the private beta by default). */
export function aiPlan(): ChatPlan {
  const p = process.env.AI_DEFAULT_PLAN?.trim().toLowerCase();
  return p === "free" || p === "pro" || p === "beta" ? p : "beta";
}

const DAY_MS = 24 * 3600 * 1000;

/** Rust `window_start`: UTC midnight, Monday 00:00 UTC, or the 1st 00:00 UTC. */
export function windowStart(w: QuotaWindow, now: Date): Date {
  const midnight = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  switch (w) {
    case "day":
      return new Date(midnight);
    case "week":
      return new Date(midnight - ((now.getUTCDay() + 6) % 7) * DAY_MS);
    case "month":
      return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
  }
}

/** Rust `window_reset`: the end (exclusive) of the window containing `now`. */
export function windowReset(w: QuotaWindow, now: Date): Date {
  const start = windowStart(w, now);
  switch (w) {
    case "day":
      return new Date(start.getTime() + DAY_MS);
    case "week":
      return new Date(start.getTime() + 7 * DAY_MS);
    case "month":
      return new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + 1, 1));
  }
}

const CAP: Record<QuotaWindow, "daily_cap" | "weekly_cap" | "monthly_cap"> = {
  day: "daily_cap",
  week: "weekly_cap",
  month: "monthly_cap",
};

export type ChatAllowance = {
  quotas: { window: QuotaWindow; limit: number; used: number; resetsAt: Date }[];
  exhausted: { reason: "weekly_cap" | "daily_cap" | "monthly_cap"; resetsAt: Date } | null;
};

/** The account's chat allowance now (the budget scope is the account, as `DefaultEntitlements`). */
export async function chatAllowance(
  db: Db,
  accountId: string,
  now: Date = new Date(),
  plan: ChatPlan = aiPlan(),
): Promise<ChatAllowance> {
  const day = windowStart("day", now);
  const week = windowStart("week", now);
  const month = windowStart("month", now);
  const earliest = new Date(Math.min(day.getTime(), week.getTime(), month.getTime()));
  const [row] = !isUuid(accountId)
    ? []
    : await readOnly(
        db,
        (tx) =>
          tx.execute(sql`
        select count(*) filter (where m.created_at >= ${day.toISOString()}::timestamptz)::int as day,
               count(*) filter (where m.created_at >= ${week.toISOString()}::timestamptz)::int as week,
               count(*) filter (where m.created_at >= ${month.toISOString()}::timestamptz)::int as month
        from coach_messages m join coach_conversations c on c.id = m.conversation_id
        where c.account_id = ${accountId}::uuid and m.role = 'assistant'
          and (m.status = 'streaming' or (m.status = 'done' and m.ai_disclosure is not null))
          and m.created_at >= ${earliest.toISOString()}::timestamptz`) as unknown as Promise<
            { day: number; week: number; month: number }[]
          >,
      );
  const used: Record<QuotaWindow, number> = {
    day: row?.day ?? 0,
    week: row?.week ?? 0,
    month: row?.month ?? 0,
  };
  const quotas = chatQuotas(plan).map((q) => ({
    window: q.window,
    limit: q.limit,
    used: used[q.window],
    resetsAt: windowReset(q.window, now),
  }));
  const first = quotas.find((q) => q.used >= q.limit);
  return {
    quotas,
    exhausted: first ? { reason: CAP[first.window], resetsAt: first.resetsAt } : null,
  };
}

/**
 * The account's chat allowance as `userId` may see it: null unless the user's own link to the
 * account is verified (holistic F10). The allowance is the account's, shared by its verified
 * users, so an unverified link never sees another user's usage.
 */
export async function coachAllowance(
  db: Db,
  userId: string,
  accountId: string,
  now: Date = new Date(),
  plan: ChatPlan = aiPlan(),
): Promise<ChatAllowance | null> {
  if (!(await isVerifiedLink(db, userId, accountId))) return null;
  return chatAllowance(db, accountId, now, plan);
}
