import {
  coachMessages,
  importRuns,
  jobs,
  reportRuns,
  reportSections,
  reports,
  userEvents,
} from "@ci/db/generated";
import { asc, eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { simulateWorker, VALIDATED_ANSWER } from "../e2e/helpers/simulate-worker";
import { resetCoachTables, seedThread } from "./helpers/coach";
import { resetCoachingTables, seedReport } from "./helpers/coaching";
import { resetGameTables, seedAccount } from "./helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "./helpers/session";

// The e2e stand-in for the R3 worker (M5b Task 11, ADR 0074): each step writes what the job
// writes and emits its event, chat events owned by the thread's user. Synthetic rows only.

const db = webTestDb();
let userId: string;
let accountId: string;

const events = () =>
  db
    .select({ type: userEvents.type, userId: userEvents.userId, payload: userEvents.payload })
    .from(userEvents)
    .orderBy(asc(userEvents.id));

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  await resetCoachTables(db);
  userId = (await createTestSession(db)).user.id;
  ({ accountId } = await seedAccount(db, { userId, username: "e2e-sim" }));
});

describe("simulateWorker: report sections", () => {
  it("claims a queued run, writes the next version, then each section with its event", async () => {
    await seedReport(db, accountId, { timeClass: "rapid", payload: "example" });
    const { runId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      run: { trigger: "manual" },
    });
    await simulateWorker(db, { kind: "report-section", runId, section: "overview" });

    const [run] = await db.select().from(reportRuns).where(eq(reportRuns.id, runId));
    expect(run?.status).toBe("running");
    const [report] = await db.select().from(reports).where(eq(reports.reportRunId, runId));
    expect(report?.version).toBe(2);
    expect(report?.status).toBe("partial");
    expect(report?.aiDisclosure).toMatchObject({ label: "Written by AI" });
    const sections = await db
      .select({ section: reportSections.section, status: reportSections.status })
      .from(reportSections)
      .where(eq(reportSections.reportId, report?.id as string));
    expect(sections).toEqual([{ section: "overview", status: "ready" }]);
    expect(
      (await events()).map((e) => [e.type, (e.payload as { section: string }).section]),
    ).toEqual([
      ["report.section.ready", "computed"],
      ["report.section.ready", "overview"],
    ]);

    for (const section of ["weaknesses", "areas", "openings", "climb_plan"] as const) {
      await simulateWorker(db, { kind: "report-section", runId, section });
    }
    const [done] = await db.select().from(reportRuns).where(eq(reportRuns.id, runId));
    expect(done?.status).toBe("done");
    const [complete] = await db.select().from(reports).where(eq(reports.reportRunId, runId));
    expect(complete?.status).toBe("complete");
    const all = await events();
    expect(all.at(-1)?.type).toBe("report.ready");
    expect(all.some((e) => (e.payload as { section?: string }).section?.startsWith("area:"))).toBe(
      true,
    );
    expect(all.every((e) => e.userId === null)).toBe(true);
  });
});

describe("simulateWorker: chat", () => {
  async function answer() {
    const { messageIds } = await seedThread(db, {
      userId,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Why?" },
        { role: "assistant", status: "pending" },
      ],
    });
    return messageIds[1] as string;
  }

  it("streams user-owned chunks, then stores the validated answer", async () => {
    const messageId = await answer();
    await simulateWorker(db, { kind: "chat-chunk", messageId, seq: 0, text: "Most misses " });
    await simulateWorker(db, { kind: "chat-chunk", messageId, seq: 1, text: "follow captures." });
    const [streaming] = await db
      .select()
      .from(coachMessages)
      .where(eq(coachMessages.id, messageId));
    expect(streaming?.status).toBe("streaming");

    await simulateWorker(db, { kind: "chat-done", messageId });
    const [done] = await db.select().from(coachMessages).where(eq(coachMessages.id, messageId));
    expect(done?.status).toBe("done");
    expect(done?.text).toBe("Most misses follow captures.");
    expect(done?.segments).toEqual([{ kind: "text", text: "Most misses follow captures." }]);
    expect(done?.aiDisclosure).toMatchObject({ label: "Written by AI", promptVersion: "chat@1" });

    const all = await events();
    expect(all.map((e) => e.type)).toEqual(["chat.chunk", "chat.chunk", "chat.message.ready"]);
    expect(all.every((e) => e.userId === userId)).toBe(true);
    expect(all[2]?.payload).toEqual({ messageId, replace: false, attempt: 1 });
    // Both chunks carry the attempt the first one reserved (Rust ChatChunk.attempt).
    expect(all.slice(0, 2).map((e) => (e.payload as { attempt: number }).attempt)).toEqual([1, 1]);
  });

  it("replace stores the validated text", async () => {
    const messageId = await answer();
    await simulateWorker(db, { kind: "chat-chunk", messageId, seq: 0, text: "draft" });
    await simulateWorker(db, { kind: "chat-done", messageId, replace: true });
    const [done] = await db.select().from(coachMessages).where(eq(coachMessages.id, messageId));
    expect(done?.text).toBe(VALIDATED_ANSWER);
    expect((await events()).at(-1)?.payload).toEqual({ messageId, replace: true, attempt: 1 });
  });

  it("a failure stores its reason and emits it once", async () => {
    const messageId = await answer();
    await simulateWorker(db, { kind: "chat-failed", messageId, reason: "weekly_cap" });
    await simulateWorker(db, { kind: "chat-failed", messageId, reason: "weekly_cap" });
    const [failed] = await db.select().from(coachMessages).where(eq(coachMessages.id, messageId));
    expect(failed?.status).toBe("failed");
    expect(failed?.failureReason).toBe("weekly_cap");
    const [{ n } = { n: 0 }] = (await db.execute(
      sql`select count(*)::int as n from user_events where type = 'chat.message.failed'`,
    )) as unknown as { n: number }[];
    expect(n).toBe(1);
  });
});

describe("simulateWorker: a failed report run", () => {
  it("fails the run with its reason and emits job.failed for the account", async () => {
    const { runId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      run: { trigger: "manual" },
    });
    await simulateWorker(db, { kind: "report-failed", runId, reason: "ai_unavailable" });
    const [run] = await db.select().from(reportRuns).where(eq(reportRuns.id, runId));
    expect(run?.status).toBe("failed");
    expect(run?.lastError).toBe("ai_unavailable");
    expect(run?.finishedAt).not.toBeNull();
    const all = await events();
    expect(all.map((e) => [e.type, e.userId])).toEqual([["job.failed", null]]);
    expect(all[0]?.payload).toEqual({ kind: "generate-report", reason: "ai_unavailable" });
  });
});

describe("simulateWorker: an incremental import", () => {
  it("finishes the account's import job and run, then announces it done", async () => {
    await db.insert(jobs).values({
      kind: "import-account",
      payload: { accountId, incremental: true },
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
    });
    await simulateWorker(db, { kind: "import-done", accountId, imported: 0 });
    const [job] = await db
      .select()
      .from(jobs)
      .where(eq(jobs.dedupeKey, `import:${accountId}`));
    expect(job?.state).toBe("done");
    const runs = await db.select().from(importRuns).where(eq(importRuns.accountId, accountId));
    expect(runs.map((r) => [r.status, r.imported, r.finishedAt !== null])).toEqual([
      ["done", 0, true],
    ]);
    const all = await events();
    expect(all.map((e) => e.type)).toEqual(["import.progress"]);
    expect(all[0]?.payload).toMatchObject({ imported: 0, done: true, runId: runs[0]?.id });
  });
});
