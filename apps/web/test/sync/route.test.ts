import { jobs } from "@ci/db/generated";
import { beforeEach, describe, expect, it } from "vitest";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import { resetSyncTables, seedGames, seedJob } from "../helpers/sync";

const db = webTestDb();
const sync = await import("../../src/app/api/sync/route");
const retry = await import("../../src/app/api/sync/retry/route");
const syncOne = await import("../../src/app/api/accounts/[linkId]/sync/route");

const ORIGIN = "http://localhost:3100";
let cookie: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetSyncTables(db);
  const session = await createTestSession(db, { email: "alice@example.invalid" });
  cookie = session.cookie;
  ({ accountId } = await seedAccount(db, { userId: session.user.id, username: "alice-route" }));
});

function request(
  path: string,
  method: "GET" | "POST",
  body?: unknown,
  opts: { origin?: string; cookie?: string | null } = {},
) {
  const headers = new Headers({
    "content-type": "application/json",
    origin: opts.origin ?? ORIGIN,
  });
  const c = opts.cookie === undefined ? cookie : opts.cookie;
  if (c) headers.set("cookie", c);
  return new Request(`${ORIGIN}${path}`, {
    method,
    headers,
    ...(body === undefined ? {} : { body: typeof body === "string" ? body : JSON.stringify(body) }),
  });
}

describe("GET /api/sync", () => {
  it("returns the user's status", async () => {
    await seedGames(db, accountId, "done", 2);
    const res = await sync.GET(request("/api/sync", "GET"));
    expect(res.status).toBe(200);
    const json = await res.json();
    expect(json.accounts).toHaveLength(1);
    expect(json.accounts[0]).toMatchObject({ accountId, analysis: { done: 2 } });
    expect(typeof json.generatedAt).toBe("string");
    expect(res.headers.get("cache-control")).toContain("no-store");
  });

  it("is 401 without a session", async () => {
    const res = await sync.GET(request("/api/sync", "GET", undefined, { cookie: null }));
    expect(res.status).toBe(401);
  });
});

describe("POST /api/accounts/:accountId/sync", () => {
  const post = (id: string, opts: { origin?: string; cookie?: string | null } = {}) =>
    syncOne.POST(request(`/api/accounts/${id}/sync`, "POST", undefined, opts), {
      params: Promise.resolve({ linkId: id }),
    });

  it("queues an incremental import of the account", async () => {
    const res = await post(accountId);
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ accountId, outcome: "queued" });
    const rows = await db.select().from(jobs);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.payload).toEqual({ accountId, incremental: true });
  });

  it("a second request while one is live is already checking", async () => {
    await post(accountId);
    const res = await post(accountId);
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ accountId, outcome: "already" });
  });

  it("is a 429 with availableAt within 2 minutes", async () => {
    await seedJob(db, {
      kind: "import-account",
      dedupeKey: `import:${accountId}`,
      userKey: accountId,
      state: "done",
    });
    const res = await post(accountId);
    expect(res.status).toBe(429);
    const json = await res.json();
    expect(json.error).toBe("throttled");
    expect(Number.isNaN(Date.parse(json.availableAt))).toBe(false);
  });

  it("refuses a cross-site post", async () => {
    const res = await post(accountId, { origin: "https://evil.example" });
    expect(res.status).toBe(403);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("a foreign or malformed account is 404", async () => {
    expect((await post("00000000-0000-4000-8000-000000000000")).status).toBe(404);
    expect((await post("nope")).status).toBe(404);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("is 401 without a session", async () => {
    expect((await post(accountId, { cookie: null })).status).toBe(401);
  });
});

describe("POST /api/sync/retry", () => {
  it("queues the failed games", async () => {
    await seedGames(db, accountId, "failed", 2);
    const res = await retry.POST(request("/api/sync/retry", "POST", { accountId }));
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ queued: 2 });
  });

  it("a foreign account is 404 and a missing one 400", async () => {
    const foreign = await retry.POST(
      request("/api/sync/retry", "POST", { accountId: "00000000-0000-4000-8000-000000000000" }),
    );
    expect(foreign.status).toBe(404);
    const missing = await retry.POST(request("/api/sync/retry", "POST", {}));
    expect(missing.status).toBe(400);
  });
});
