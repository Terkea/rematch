// Port of the Agent SDK's SessionStore conformance suite (examples/session-stores/shared/conformance.ts,
// not shipped in the npm package): the 13 contracts, re-written for vitest. Session ids are UUIDs here
// because `ai_sdk_session_entries.session_id` is a uuid column. Plus dedup, ordering under concurrency
// and the role's grants.
import { randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import pg from "pg";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { PgSessionStore } from "../src/session-store.js";

const ADMIN_URL = process.env.DATABASE_URL_TEST;
const MIGRATION = fileURLToPath(new URL("./fixtures/ai_sdk_session_entries.sql", import.meta.url));
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
  const dbName = `ai_sidecar_store_${randomBytes(4).toString("hex")}`;
  const password = randomBytes(12).toString("hex");
  let admin: pg.Client;
  let dbAdmin: pg.Client;
  let pool: pg.Pool;
  let store: PgSessionStore;

  beforeAll(async () => {
    admin = new pg.Client({ connectionString: ADMIN_URL });
    await admin.connect();
    await admin.query(`CREATE DATABASE ${dbName}`);
    const url = new URL(ADMIN_URL as string);
    url.pathname = `/${dbName}`;
    dbAdmin = new pg.Client({ connectionString: url.toString() });
    await dbAdmin.connect();
    await dbAdmin.query(readFileSync(MIGRATION, "utf8"));
    await dbAdmin.query(`ALTER ROLE ai_sidecar PASSWORD '${password}'`);
    await dbAdmin.query("CREATE TABLE other_secret (x int)");
    url.username = "ai_sidecar";
    url.password = password;
    pool = new pg.Pool({ connectionString: url.toString(), max: 4 });
    store = new PgSessionStore(pool);
  });

  afterAll(async () => {
    await pool?.end();
    await dbAdmin?.end();
    await admin?.query(`DROP DATABASE IF EXISTS ${dbName} WITH (FORCE)`);
    await admin?.end();
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
