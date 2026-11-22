import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { aiPlan, chatAllowance, chatQuotas } from "../../src/server/coach/allowance.js";
import { resetCoachTables, seedThread } from "../helpers/coach";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let userId: string;
let accountId: string;

// Wednesday 18 Nov 2026, 15:00 UTC: the week started Monday 16 Nov 00:00 UTC.
const NOW = new Date("2026-11-18T15:00:00Z");
const MONDAY = new Date("2026-11-16T00:00:00Z");
const NEXT_MONDAY = new Date("2026-11-23T00:00:00Z");
const at = (iso: string) => new Date(iso);
const text = [{ kind: "text" as const, text: "A" }];

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "quota" })).accountId;
});

const env = { ...process.env };
afterEach(() => {
  process.env = { ...env };
});

describe("chatAllowance", () => {
  it("counts streaming and AI-written answers this week (free plan)", async () => {
    await seedThread(db, {
      userId,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q", createdAt: at("2026-11-16T09:00:00Z") },
        {
          role: "assistant",
          status: "done",
          segments: text,
          createdAt: at("2026-11-16T09:00:05Z"),
        },
        { role: "user", status: "done", text: "Q", createdAt: at("2026-11-17T09:00:00Z") },
        {
          role: "assistant",
          status: "done",
          segments: text,
          createdAt: at("2026-11-17T09:00:05Z"),
        },
        { role: "user", status: "done", text: "Q", createdAt: at("2026-11-18T14:00:00Z") },
        { role: "assistant", status: "streaming", createdAt: at("2026-11-18T14:00:05Z") },
      ],
    });
    const r = await chatAllowance(db, accountId, NOW, "free");
    expect(r.quotas).toEqual([{ window: "week", limit: 3, used: 3, resetsAt: NEXT_MONDAY }]);
    expect(r.exhausted).toEqual({ reason: "weekly_cap", resetsAt: NEXT_MONDAY });
  });

  it("does not count no-data, failed, pending or last week's answers", async () => {
    await seedThread(db, {
      userId,
      accountId,
      timeClass: "blitz",
      messages: [
        // Fixed no-data copy: done without a disclosure.
        {
          role: "assistant",
          status: "done",
          segments: text,
          aiDisclosure: null,
          createdAt: at("2026-11-17T10:00:00Z"),
        },
        {
          role: "assistant",
          status: "failed",
          failureReason: "weekly_cap",
          createdAt: at("2026-11-17T11:00:00Z"),
        },
        { role: "assistant", status: "pending", createdAt: at("2026-11-18T14:59:00Z") },
        // Sunday 23:59 of last week.
        {
          role: "assistant",
          status: "done",
          segments: text,
          createdAt: at("2026-11-15T23:59:59Z"),
        },
        // Exactly Monday 00:00 counts.
        { role: "assistant", status: "done", segments: text, createdAt: MONDAY },
      ],
    });
    const r = await chatAllowance(db, accountId, NOW, "free");
    expect(r.quotas[0]?.used).toBe(1);
    expect(r.exhausted).toBeNull();
  });

  it("counts only this account's conversations", async () => {
    const other = (await seedAccount(db, { userId, username: "other" })).accountId;
    await seedThread(db, {
      userId,
      accountId: other,
      timeClass: "rapid",
      messages: [{ role: "assistant", status: "done", segments: text, createdAt: NOW }],
    });
    expect((await chatAllowance(db, accountId, NOW, "free")).quotas[0]?.used).toBe(0);
    expect((await chatAllowance(db, other, NOW, "free")).quotas[0]?.used).toBe(1);
  });

  it("pro counts per day and month and reports the first exhausted", async () => {
    expect(chatQuotas("pro")).toEqual([
      { window: "day", limit: 30 },
      { window: "month", limit: 300 },
    ]);
    const r = await chatAllowance(db, accountId, NOW, "pro");
    expect(r.quotas).toEqual([
      { window: "day", limit: 30, used: 0, resetsAt: at("2026-11-19T00:00:00Z") },
      { window: "month", limit: 300, used: 0, resetsAt: at("2026-12-01T00:00:00Z") },
    ]);
    expect(r.exhausted).toBeNull();
  });

  it("the beta plan (ruling G1) is 100 a week unless configured", () => {
    delete process.env.AI_DEFAULT_PLAN;
    delete process.env.AI_BETA_CHAT_WEEKLY;
    expect(aiPlan()).toBe("beta");
    expect(chatQuotas("beta")).toEqual([{ window: "week", limit: 100 }]);
    process.env.AI_BETA_CHAT_WEEKLY = "40";
    expect(chatQuotas("beta")).toEqual([{ window: "week", limit: 40 }]);
    process.env.AI_BETA_CHAT_WEEKLY = "lots";
    expect(chatQuotas("beta")).toEqual([{ window: "week", limit: 100 }]);
    process.env.AI_DEFAULT_PLAN = "free";
    expect(aiPlan()).toBe("free");
    process.env.AI_DEFAULT_PLAN = "platinum";
    expect(aiPlan()).toBe("beta");
  });

  it("a December month window resets on 1 January", async () => {
    const r = await chatAllowance(db, accountId, at("2026-12-31T23:00:00Z"), "pro");
    expect(r.quotas[1]?.resetsAt).toEqual(at("2027-01-01T00:00:00Z"));
  });
});
