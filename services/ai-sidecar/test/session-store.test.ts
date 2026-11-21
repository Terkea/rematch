// Port of the Agent SDK's SessionStore conformance suite (examples/session-stores/shared/conformance.ts,
// not shipped in the npm package): the 13 contracts, re-written for vitest. Session ids are UUIDs here
// because `ai_sdk_session_entries.session_id` is a uuid column. Plus dedup, ordering under concurrency,
// NUL handling and the role's grants.
import { randomUUID } from "node:crypto";
import type pg from "pg";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { runChat } from "../src/chat.js";
import { Limiter } from "../src/limiter.js";
import { parseChatRequest } from "../src/options.js";
import { PgSessionStore, TURN_LOCK_CLASS } from "../src/session-store.js";
import { createStoreDb, type StoreDb } from "./db.js";
import { captureLogger, FakeRes, result, testConfig, textDelta } from "./helpers.js";

const ADMIN_URL = process.env.DATABASE_URL_TEST;
if (!ADMIN_URL && process.env.IN_DOCKER === "1") {
  throw new Error("DATABASE_URL_TEST is required with IN_DOCKER=1");
}

const S = randomUUID(); // 'sess'
const S1 = randomUUID();
const S2 = randomUUID();
const S3 = randomUUID();
const OTHER = randomUUID();
const KEY = { projectKey: "proj", sessionId: S };
const e = (type: string, extra: Record<string, unknown> = {}) => ({ type, ...extra });

/** Deep equality that ignores object key order (jsonb reorders keys), as the SDK suite does. */
function canon(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(canon);
  if (v && typeof v === "object") {
    return Object.fromEntries(
      Object.keys(v)
        .sort()
        .map((k) => [k, canon((v as Record<string, unknown>)[k])]),
    );
  }
  return v;
}

describe.skipIf(!ADMIN_URL)("PgSessionStore", () => {
  let db: StoreDb;
  let dbAdmin: pg.Client;
  let pool: pg.Pool;
  let store: PgSessionStore;

  beforeAll(async () => {
    db = await createStoreDb(ADMIN_URL as string);
    ({ dbAdmin, pool } = db);
    await dbAdmin.query("CREATE TABLE other_secret (x int)");
    store = new PgSessionStore(pool);
  });

  afterAll(async () => {
    await db?.close();
  });

  beforeEach(async () => {
    await dbAdmin.query("TRUNCATE ai_sdk_session_entries");
  });

  it("append_then_load_returns_same_entries_in_same_order", async () => {
    const entries = [e("a", { n: 1, nested: { x: [1, 2] } }), e("b", { n: 2 })];
    await store.append(KEY, entries);
    expect(canon(await store.load(KEY))).toEqual(canon(entries));
  });

  it("load_unknown_key_returns_null", async () => {
    expect(await store.load(KEY)).toBeNull();
    expect(await store.load({ ...KEY, subpath: "subagents/a" })).toBeNull();
  });

  it("multiple_append_calls_preserve_call_order", async () => {
    await store.append(KEY, [e("a")]);
    await store.append(KEY, [e("b"), e("c")]);
    await store.append(KEY, [e("d")]);
    expect(await store.load(KEY)).toEqual([e("a"), e("b"), e("c"), e("d")]);
  });

  it("append_empty_is_a_noop", async () => {
    await store.append(KEY, []);
    expect(await store.load(KEY)).toBeNull();
    await store.append(KEY, [e("a")]);
    await store.append(KEY, []);
    expect(await store.load(KEY)).toEqual([e("a")]);
  });

  it("subpath_keys_are_stored_independently_of_main", async () => {
    await store.append(KEY, [e("main")]);
    await store.append({ ...KEY, subpath: "subagents/x" }, [e("sub")]);
    expect(await store.load(KEY)).toEqual([e("main")]);
    expect(await store.load({ ...KEY, subpath: "subagents/x" })).toEqual([e("sub")]);
  });

  it("project_key_isolation", async () => {
    await store.append({ projectKey: "A", sessionId: S }, [e("a")]);
    await store.append({ projectKey: "B", sessionId: S }, [e("b")]);
    expect(await store.load({ projectKey: "A", sessionId: S })).toEqual([e("a")]);
    expect(await store.load({ projectKey: "B", sessionId: S })).toEqual([e("b")]);
  });

  it("list_sessions_returns_session_ids_for_project", async () => {
    await store.append({ projectKey: "P", sessionId: S1 }, [e("a")]);
    await store.append({ projectKey: "P", sessionId: S2 }, [e("a")]);
    await store.append({ projectKey: "Q", sessionId: S3 }, [e("a")]);
    const list = await store.listSessions("P");
    expect(list.map((s) => s.sessionId).sort()).toEqual([S1, S2].sort());
    for (const s of list) {
      expect(s.mtime).toBeGreaterThan(1e12);
      expect(Number.isInteger(s.mtime)).toBe(true);
    }
    expect(await store.listSessions("never-seen")).toEqual([]);
  });

  it("list_sessions_excludes_subagent_subpaths", async () => {
    await store.append({ projectKey: "P", sessionId: S1, subpath: "subagents/x" }, [e("a")]);
    expect((await store.listSessions("P")).map((s) => s.sessionId)).not.toContain(S1);
  });

  it("delete_main_then_load_returns_null", async () => {
    await store.append(KEY, [e("a")]);
    await store.delete(KEY);
    expect(await store.load(KEY)).toBeNull();
    await expect(store.delete({ projectKey: "proj", sessionId: randomUUID() })).resolves.toBe(
      undefined,
    );
  });

  it("delete_main_cascades_to_subkeys", async () => {
    await store.append(KEY, [e("main")]);
    await store.append({ ...KEY, subpath: "subagents/a" }, [e("sa")]);
    await store.append({ ...KEY, subpath: "subagents/b" }, [e("sb")]);
    await store.append({ projectKey: "proj", sessionId: OTHER }, [e("o")]);
    await store.append({ projectKey: "proj2", sessionId: S }, [e("p2")]);
    await store.delete(KEY);
    expect(await store.load(KEY)).toBeNull();
    expect(await store.load({ ...KEY, subpath: "subagents/a" })).toBeNull();
    expect(await store.load({ ...KEY, subpath: "subagents/b" })).toBeNull();
    expect(await store.load({ projectKey: "proj", sessionId: OTHER })).toEqual([e("o")]);
    expect(await store.load({ projectKey: "proj2", sessionId: S })).toEqual([e("p2")]);
    expect(await store.listSubkeys(KEY)).toEqual([]);
  });

  it("delete_with_subpath_removes_only_that_subkey", async () => {
    await store.append(KEY, [e("main")]);
    await store.append({ ...KEY, subpath: "subagents/a" }, [e("sa")]);
    await store.append({ ...KEY, subpath: "subagents/b" }, [e("sb")]);
    await store.delete({ ...KEY, subpath: "subagents/a" });
    expect(await store.load(KEY)).toEqual([e("main")]);
    expect(await store.load({ ...KEY, subpath: "subagents/a" })).toBeNull();
    expect(await store.load({ ...KEY, subpath: "subagents/b" })).toEqual([e("sb")]);
  });

  it("list_subkeys_returns_subpaths_for_the_session", async () => {
    await store.append({ ...KEY, subpath: "subagents/a" }, [e("a")]);
    await store.append({ ...KEY, subpath: "subagents/b" }, [e("b")]);
    await store.append({ projectKey: "proj", sessionId: OTHER, subpath: "subagents/c" }, [e("c")]);
    expect((await store.listSubkeys(KEY)).sort()).toEqual(["subagents/a", "subagents/b"]);
  });

  it("list_subkeys_excludes_main_transcript", async () => {
    await store.append(KEY, [e("main")]);
    expect(await store.listSubkeys(KEY)).toEqual([]);
    expect(await store.listSubkeys({ projectKey: "x", sessionId: randomUUID() })).toEqual([]);
  });

  // ---- beyond the SDK suite ------------------------------------------------

  it("retried_batches_dedupe_by_uuid_and_keep_entries_without_one", async () => {
    const u1 = randomUUID();
    await store.append(KEY, [e("user", { uuid: u1 }), e("title")]);
    await store.append(KEY, [e("user", { uuid: u1 }), e("title"), e("assistant", { uuid: "u2" })]);
    expect(await store.load(KEY)).toEqual([
      e("user", { uuid: u1 }),
      e("title"),
      e("title"),
      e("assistant", { uuid: "u2" }),
    ]);
  });

  it("concurrent_appends_keep_each_batch_contiguous", async () => {
    await Promise.all(
      Array.from({ length: 8 }, (_, i) => store.append(KEY, [e(`b${i}-0`), e(`b${i}-1`)])),
    );
    const types = ((await store.load(KEY)) ?? []).map((x) => x.type);
    expect(types).toHaveLength(16);
    for (let i = 0; i < types.length; i += 2) {
      expect(types[i]?.replace("-0", "")).toBe(types[i + 1]?.replace("-1", ""));
    }
  });

  it("has_session_and_delete_session_cover_every_project_and_subpath", async () => {
    await store.append(KEY, [e("main")]);
    await store.append({ ...KEY, subpath: "subagents/a" }, [e("sa")]);
    expect(await store.hasSession(S)).toBe(true);
    await store.deleteSession(S);
    expect(await store.hasSession(S)).toBe(false);
    const { rows } = await dbAdmin.query(
      "SELECT count(*)::int AS n FROM ai_sdk_session_entries WHERE session_id = $1",
      [S],
    );
    expect(rows[0].n).toBe(0);
  });

  it("nul_characters_are_stripped_and_counted", async () => {
    let stripped = 0;
    const counting = new PgSessionStore(pool, { onNulStripped: (n) => (stripped += n) });
    await counting.append(KEY, [
      e("user", { text: "a\u0000b", "k\u0000": ["x\u0000"] }),
      e("plain", { text: "literal \\u0000 stays" }),
    ]);
    expect(await counting.load(KEY)).toEqual([
      e("user", { text: "ab", k: ["x"] }),
      e("plain", { text: "literal \\u0000 stays" }),
    ]);
    expect(stripped).toBe(1);
  });

  it("a_batch_repeating_a_uuid_keeps_the_first", async () => {
    await store.append(KEY, [e("user", { uuid: "dup" }), e("assistant", { uuid: "dup" }), e("t")]);
    expect(await store.load(KEY)).toEqual([e("user", { uuid: "dup" }), e("t")]);
  });

  it("the_conversation_lock_excludes_a_second_holder_until_released", async () => {
    const unlock = await store.tryLockConversation(S);
    expect(unlock).not.toBeNull();
    // A second store (another replica) cannot take it.
    const replica = new PgSessionStore(pool);
    expect(await replica.tryLockConversation(S)).toBeNull();
    // Other conversations are unaffected.
    const other = await replica.tryLockConversation(OTHER);
    expect(other).not.toBeNull();
    await other?.();
    await unlock?.();
    const again = await replica.tryLockConversation(S);
    expect(again).not.toBeNull();
    await again?.();
    // Every lock connection went back to the pool.
    expect(pool.totalCount - pool.idleCount).toBe(0);
  });

  it("a_terminated_lock_connection_is_survived_and_the_turn_finishes", async () => {
    let lost = 0;
    const lockStore = new PgSessionStore(pool, { onLockLost: () => (lost += 1) });
    const conv = randomUUID();
    let terminated = 0;
    const query = (() =>
      (async function* () {
        yield textDelta("before");
        // Kill the backend that holds this conversation's turn lock (57P01 on the client).
        const r = await dbAdmin.query(
          `SELECT pg_terminate_backend(pid) AS ok FROM pg_locks
            WHERE locktype = 'advisory' AND classid = $1 AND granted`,
          [TURN_LOCK_CLASS],
        );
        terminated = r.rowCount ?? 0;
        await new Promise((res) => setTimeout(res, 300)); // let the client see the error
        yield textDelta("after");
        yield result();
      })()) as never;
    const chatBody = {
      model: "claude-sonnet-5-5",
      effort: "low",
      maxTokens: 100,
      timeoutMs: 30000,
      maxTurns: 2,
      systemPrompt: "s",
      prompt: [{ type: "text", text: "hi" }],
      conversationId: conv,
      tools: [],
      toolCallback: { url: "http://127.0.0.1:9/v1/tools", token: "t" },
    };
    const d = {
      limiter: new Limiter(2, 2, 1000),
      logger: captureLogger(),
      store: lockStore,
      turns: new Set<string>(),
      query,
    };
    const res = new FakeRes();
    await runChat(parseChatRequest(chatBody), testConfig(), res, d);
    expect(terminated).toBe(1);
    expect(lost).toBe(1);
    // The process survived (an unhandled 'error' would fail this run) and the turn finished.
    expect(res.events().map((e) => e.event)).toEqual(["delta", "delta", "done"]);
    // The lock is free again (the server dropped it with the connection), and a new turn works.
    const again = new FakeRes();
    await runChat(parseChatRequest(chatBody), testConfig(), again, {
      ...d,
      query: (() =>
        (async function* () {
          yield result();
        })()) as never,
    });
    expect(again.events().map((e) => e.event)).toEqual(["done"]);
    expect(lost).toBe(1);
    await vi.waitFor(() => expect(pool.totalCount - pool.idleCount).toBe(0));
  });

  it("the_role_reaches_only_its_table", async () => {
    await expect(pool.query("SELECT * FROM other_secret")).rejects.toThrow(/permission denied/);
    await expect(
      pool.query("UPDATE ai_sdk_session_entries SET entry = '{}'::jsonb"),
    ).rejects.toThrow(/permission denied/);
    await expect(pool.query("TRUNCATE ai_sdk_session_entries")).rejects.toThrow(
      /permission denied/,
    );
  });
});
