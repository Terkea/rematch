import { reportRuns } from "@ci/db/generated";
import { beforeEach, describe, expect, it } from "vitest";
import { resetCoachingTables, seedReport } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const { POST } = await import("../../src/app/api/coach/report/route");

const ORIGIN = "http://localhost:3100";
const DAY = 86_400_000;
let cookie: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  const session = await createTestSession(db, { email: "alice@example.invalid" });
  cookie = session.cookie;
  ({ accountId } = await seedAccount(db, { userId: session.user.id, username: "alice-route" }));
});

function post(body: unknown, opts: { origin?: string; cookie?: string | null } = {}) {
  const headers = new Headers({
    "content-type": "application/json",
    origin: opts.origin ?? ORIGIN,
  });
  const c = opts.cookie === undefined ? cookie : opts.cookie;
  if (c) headers.set("cookie", c);
  return POST(
    new Request(`${ORIGIN}/api/coach/report`, {
      method: "POST",
      headers,
      body: typeof body === "string" ? body : JSON.stringify(body),
    }),
  );
}

describe("POST /api/coach/report", () => {
  it("a cross-site post is forbidden", async () => {
    const res = await post({ accountId, timeClass: "rapid" }, { origin: "https://evil.example" });
    expect(res.status).toBe(403);
    expect(await db.select().from(reportRuns)).toHaveLength(0);
  });

  it("a bad body is a 400", async () => {
    for (const body of ["{not json", { accountId }, { accountId, timeClass: "hyper" }, [1]]) {
      const res = await post(body);
      expect(res.status).toBe(400);
      expect(await res.json()).toEqual({ error: "invalid_body" });
    }
  });

  it("signed out is a 401", async () => {
    const res = await post({ accountId, timeClass: "rapid" }, { cookie: null });
    expect(res.status).toBe(401);
  });

  it("queues a report, then reports the live run", async () => {
    const first = await post({ accountId, timeClass: "rapid" });
    expect(first.status).toBe(202);
    const { runId } = (await first.json()) as { runId: string };
    expect(runId).toMatch(/^[0-9a-f-]{36}$/);
    const second = await post({ accountId, timeClass: "rapid" });
    expect(second.status).toBe(202);
    expect(await second.json()).toEqual({ runId, building: true });
  });

  it("inside the interval is a 429 with the date", async () => {
    const createdAt = new Date(Date.now() - 10 * DAY);
    await seedReport(db, accountId, { timeClass: "rapid", payload: "example", createdAt });
    const res = await post({ accountId, timeClass: "rapid" });
    expect(res.status).toBe(429);
    expect(await res.json()).toEqual({
      error: "plan_interval",
      availableAt: new Date(createdAt.getTime() + 30 * DAY).toISOString(),
    });
  });

  it("a foreign or malformed account is a 404", async () => {
    for (const id of ["00000000-0000-4000-8000-000000000000", "not-a-uuid"]) {
      const res = await post({ accountId: id, timeClass: "rapid" });
      expect(res.status).toBe(404);
      expect(await res.json()).toEqual({ error: "not_found" });
    }
  });
});
