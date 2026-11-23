import { accountLinks, coachConversations, reportRuns } from "@ci/db/generated";
import { and, eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { resetEnvCache } from "../../src/env";
import { coachAllowance } from "../../src/server/coach/allowance";
import { changeNotes, getReport, listWeeklyReviews } from "../../src/server/coaching";
import { resetCoachTables } from "../helpers/coach";
import { resetCoachingTables, seedNote, seedReport, seedWeekly } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// Holistic F10 (the owner's ruling, "analysis only"): a user whose link to an account is not
// verified sees the computed analysis of its public games, but none of the account's AI
// artefacts (the report, the change notes, the weekly reviews, the coach allowance), and cannot
// ask the coach or build a report until the link is verified. Server-side, not only in the UI.

const db = webTestDb();
const threads = await import("../../src/app/api/coach/threads/route");
const report = await import("../../src/app/api/coach/report/route");
const ORIGIN = "http://localhost:3100";

let owner: string;
let stranger: { id: string; cookie: string };
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  await resetCoachTables(db);
  owner = (await createTestSession(db, { email: "owner@example.invalid" })).user.id;
  const s = await createTestSession(db, { email: "stranger@example.invalid" });
  stranger = { id: s.user.id, cookie: s.cookie };
  ({ accountId } = await seedAccount(db, { userId: owner, username: "shared", verified: true }));
  // The stranger linked the same public username without verifying it.
  await db
    .insert(accountLinks)
    .values({ userId: stranger.id, linkedAccountId: accountId, method: "username" });
  await seedReport(db, accountId, { timeClass: "rapid", payload: "example" });
  await seedNote(db, accountId, { changeKey: "W1:onset:2026-10-02", seriesKey: "W1" });
  await seedWeekly(db, accountId, { timeClass: "rapid", weekStart: "2026-11-09" });
});

const scope = () => ({ accountId, timeClass: "rapid" as const });

function post(path: string, body: unknown, cookie: string) {
  return new Request(`${ORIGIN}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", origin: ORIGIN, cookie },
    body: JSON.stringify(body),
  });
}

describe("an unverified link sees analysis only", () => {
  it("the verified owner reads the AI artefacts", async () => {
    expect((await getReport(db, owner, scope())).payload).not.toBeNull();
    expect(await changeNotes(db, owner, accountId)).toHaveLength(1);
    expect(await listWeeklyReviews(db, owner, scope())).toHaveLength(1);
    expect(await coachAllowance(db, owner, accountId)).not.toBeNull();
  });

  it("gets no report, only the locked state", async () => {
    const got = await getReport(db, stranger.id, scope());
    expect(got.payload).toBeNull();
    expect(got.locked).toBe("unverified");
  });

  it("gets no change notes, weekly reviews or allowance", async () => {
    expect(await changeNotes(db, stranger.id, accountId)).toEqual([]);
    expect(await changeNotes(db, stranger.id, accountId, ["W1"])).toEqual([]);
    expect(await listWeeklyReviews(db, stranger.id, scope())).toEqual([]);
    expect(await coachAllowance(db, stranger.id, accountId)).toBeNull();
  });

  it("cannot start a conversation through the API", async () => {
    for (const body of [
      { accountId, timeClass: "rapid" },
      { accountId, timeClass: "rapid", question: "Why do I hang pieces?" },
    ]) {
      const res = await threads.POST(post("/api/coach/threads", body, stranger.cookie));
      expect(res.status).toBe(403);
      expect(await res.json()).toEqual({ error: "not_verified" });
    }
    const made = await db
      .select()
      .from(coachConversations)
      .where(eq(coachConversations.userId, stranger.id));
    expect(made).toHaveLength(0);
  });

  it("cannot build a report through the API", async () => {
    const before = await db.select().from(reportRuns).where(eq(reportRuns.accountId, accountId));
    const res = await report.POST(
      post("/api/coach/report", { accountId, timeClass: "rapid" }, stranger.cookie),
    );
    expect(res.status).toBe(403);
    expect(await res.json()).toMatchObject({ error: "not_verified" });
    const after = await db.select().from(reportRuns).where(eq(reportRuns.accountId, accountId));
    expect(after).toHaveLength(before.length);
  });

  it("sees it all once the link is verified", async () => {
    // One verified owner per account: verifying the stranger's link moves ownership.
    await db.update(accountLinks).set({ verifiedAt: null }).where(eq(accountLinks.userId, owner));
    await db
      .update(accountLinks)
      .set({ verifiedAt: new Date() })
      .where(
        and(eq(accountLinks.userId, stranger.id), eq(accountLinks.linkedAccountId, accountId)),
      );
    expect((await getReport(db, stranger.id, scope())).payload).not.toBeNull();
    expect(await coachAllowance(db, stranger.id, accountId)).not.toBeNull();
  });
});

// The owner: "bypass that for local envs". With ALLOW_UNVERIFIED_LINKS (refused in production),
// an unverified link is treated as verified, so a locally linked account (dev:link, no chess.com
// Location code) still sees its own report and coach. Without the flag the gate above holds.
describe("ALLOW_UNVERIFIED_LINKS (local only)", () => {
  const withFlag = async (fn: () => Promise<void>) => {
    process.env.ALLOW_UNVERIFIED_LINKS = "1";
    resetEnvCache();
    try {
      await fn();
    } finally {
      delete process.env.ALLOW_UNVERIFIED_LINKS;
      resetEnvCache();
    }
  };

  it("lets an unverified link read the AI artefacts and ask the coach", async () => {
    await withFlag(async () => {
      expect((await getReport(db, stranger.id, scope())).payload).not.toBeNull();
      expect(await changeNotes(db, stranger.id, accountId)).toHaveLength(1);
      expect(await listWeeklyReviews(db, stranger.id, scope())).toHaveLength(1);
      expect(await coachAllowance(db, stranger.id, accountId)).not.toBeNull();
      const res = await threads.POST(
        post("/api/coach/threads", { accountId, timeClass: "rapid" }, stranger.cookie),
      );
      expect(res.status).not.toBe(403);
    });
  });

  it("is off by default: the gate holds without the flag", async () => {
    expect((await getReport(db, stranger.id, scope())).locked).toBe("unverified");
  });
});
