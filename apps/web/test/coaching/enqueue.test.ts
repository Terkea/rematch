import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { jobs } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { enqueueJob, JOB } from "../../src/server/jobs";
import { assertSeedable } from "../helpers/games";
import { resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await assertSeedable(db);
  await db.execute(sql`truncate table jobs`);
});

const accountId = randomUUID();

describe("enqueueJob", () => {
  it("enqueues with the R1 columns", async () => {
    const runId = randomUUID();
    const id = await enqueueJob(db, {
      kind: JOB.report.kind,
      payload: { reportRunId: runId },
      dedupeKey: JOB.report.key(runId),
      userKey: accountId,
      priority: JOB.report.priority,
      maxAttempts: JOB.report.maxAttempts,
    });
    expect(typeof id).toBe("number");
    const [row] = await db
      .select()
      .from(jobs)
      .where(eq(jobs.id, BigInt(id ?? -1)));
    expect(row).toMatchObject({
      kind: "generate-report",
      payload: { reportRunId: runId },
      dedupeKey: `report:${runId}`,
      userKey: accountId,
      priority: 50,
      maxAttempts: 3,
      state: "queued",
      recurEvery: null,
    });
  });

  it("a live duplicate is skipped", async () => {
    const job = {
      kind: JOB.chat.kind,
      payload: { messageId: "m1" },
      dedupeKey: JOB.chat.key("m1"),
      userKey: accountId,
      priority: JOB.chat.priority,
      maxAttempts: JOB.chat.maxAttempts,
    };
    expect(await enqueueJob(db, job)).not.toBeNull();
    expect(await enqueueJob(db, job)).toBeNull();
    const rows = await db.select().from(jobs);
    expect(rows).toHaveLength(1);
  });

  it("a finished duplicate is enqueued again", async () => {
    const job = {
      kind: JOB.report.kind,
      payload: { reportRunId: "r1" },
      dedupeKey: JOB.report.key("r1"),
      userKey: accountId,
      priority: JOB.report.priority,
      maxAttempts: JOB.report.maxAttempts,
    };
    const first = await enqueueJob(db, job);
    await db.execute(sql`update jobs set state = 'done', finished_at = now() where id = ${first}`);
    const second = await enqueueJob(db, job);
    expect(second).not.toBeNull();
    expect(second).not.toBe(first);
    expect(await db.select().from(jobs)).toHaveLength(2);
  });
});

// The constants and payload field names are what `backend/crates/ai/src/jobs/kinds.rs` expects.
describe("job contract with the Rust R3 jobs", () => {
  const kinds = readFileSync(
    new URL("../../../../backend/crates/ai/src/jobs/kinds.rs", import.meta.url),
    "utf8",
  );
  const constant = (name: string) =>
    kinds.match(new RegExp(`pub const ${name}: [^=]+= ([^;]+);`))?.[1];

  it("kinds, priorities and attempts match kinds.rs", () => {
    expect(constant("KIND_GENERATE_REPORT")).toBe(JSON.stringify(JOB.report.kind));
    expect(constant("KIND_COACH_CHAT")).toBe(JSON.stringify(JOB.chat.kind));
    expect(constant("REPORT_PRIORITY")).toBe(String(JOB.report.priority));
    expect(constant("REPORT_MAX_ATTEMPTS")).toBe(String(JOB.report.maxAttempts));
    expect(constant("CHAT_PRIORITY")).toBe(String(JOB.chat.priority));
    expect(constant("CHAT_MAX_ATTEMPTS")).toBe(String(JOB.chat.maxAttempts));
  });

  it("dedupe keys and payload fields match kinds.rs", () => {
    expect(kinds).toContain('format!("report:{run_id}")');
    expect(kinds).toContain('format!("chat:{message_id}")');
    expect(JOB.report.key("x")).toBe("report:x");
    expect(JOB.chat.key("y")).toBe("chat:y");
    // camelCase serde of `report_run_id` / `message_id`.
    expect(kinds).toMatch(/struct GenerateReportPayload \{\s*pub report_run_id: Uuid,\s*\}/);
    expect(kinds).toMatch(/struct CoachChatPayload \{\s*pub message_id: Uuid,\s*\}/);
    expect(JOB.report.payload("x")).toEqual({ reportRunId: "x" });
    expect(JOB.chat.payload("y")).toEqual({ messageId: "y" });
  });

  it("the insert mirrors ENQUEUE_SQL", () => {
    const queue = readFileSync(
      new URL("../../../../backend/crates/queue/src/queue.rs", import.meta.url),
      "utf8",
    );
    expect(queue).toContain(
      "INSERT INTO jobs (kind, payload, dedupe_key, user_key, priority, run_at, max_attempts, recur_every)",
    );
    expect(queue).toContain(
      "ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND state IN ('queued','running','paused')",
    );
  });
});
