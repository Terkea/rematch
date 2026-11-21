import type { SessionKey, SessionStoreEntry } from "@anthropic-ai/claude-agent-sdk";
import type pg from "pg";
import type { ChatSessionStore } from "./chat.js";

/**
 * The Agent SDK `SessionStore` on Postgres (ADR-R3-6). It touches one table,
 * `ai_sdk_session_entries` (its migration is owned by Task 17), as the `ai_sidecar` role, which may only
 * SELECT, INSERT and DELETE there.
 *
 * Entries are opaque: they go to Postgres as one JSON string and come back as jsonb values, and the
 * sidecar never inspects them. The only field SQL reads is the SDK-documented top-level `uuid`,
 * which the docs ask adapters to use as an idempotency key for retried batches.
 *
 * Rust owns deletion of a conversation: it deletes this table's rows for the conversation id in the
 * same transaction as the conversation (Tasks 17, 25). The sidecar deletes only to clear a
 * transcript it could not resume, before rebuilding it.
 */
const T = "ai_sdk_session_entries";

/**
 * Postgres text and jsonb cannot hold U+0000. Strings and keys keep everything else; the count of
 * affected entries goes to a metric, so a lossy transcript is visible.
 */
function stripNul(v: unknown): { value: unknown; changed: boolean } {
  if (typeof v === "string") {
    return v.includes("\u0000")
      ? { value: v.replaceAll("\u0000", ""), changed: true }
      : { value: v, changed: false };
  }
  if (Array.isArray(v)) {
    let changed = false;
    const value = v.map((x) => {
      const r = stripNul(x);
      changed ||= r.changed;
      return r.value;
    });
    return { value, changed };
  }
  if (v && typeof v === "object") {
    let changed = false;
    const out: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) {
      const key = stripNul(k);
      const r = stripNul(x);
      changed ||= key.changed || r.changed;
      out[key.value as string] = r.value;
    }
    return { value: out, changed };
  }
  return { value: v, changed: false };
}

export type PgSessionStoreOptions = {
  /** Called with the number of entries in a batch that had U+0000 removed. */
  onNulStripped?: (entries: number) => void;
};

export class PgSessionStore implements ChatSessionStore {
  constructor(
    private readonly pool: pg.Pool,
    private readonly opts: PgSessionStoreOptions = {},
  ) {}

  async append(key: SessionKey, entries: SessionStoreEntry[]): Promise<void> {
    if (entries.length === 0) return;
    let stripped = 0;
    const clean = entries.map((e) => {
      const r = stripNul(e);
      if (r.changed) stripped += 1;
      return r.value;
    });
    if (stripped > 0) this.opts.onNulStripped?.(stripped);
    const c = await this.pool.connect();
    try {
      await c.query("BEGIN");
      // Serialize appends per transcript so each batch gets a contiguous run of seq numbers. A hash
      // collision only makes two transcripts share a lock; it never mixes their rows.
      await c.query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [
        JSON.stringify([key.projectKey, key.sessionId, key.subpath ?? ""]),
      ]);
      await c.query(
        `INSERT INTO ${T} (project_key, session_id, subpath, seq, entry)
         SELECT $1, $2::uuid, $3, base.m + row_number() OVER (ORDER BY e.ord), e.value
         FROM jsonb_array_elements($4::jsonb) WITH ORDINALITY AS e(value, ord),
              (SELECT coalesce(max(seq), 0) AS m FROM ${T}
                WHERE project_key = $1 AND session_id = $2::uuid AND subpath = $3) AS base
         WHERE e.value->>'uuid' IS NULL OR NOT EXISTS (
           SELECT 1 FROM ${T} x
            WHERE x.project_key = $1 AND x.session_id = $2::uuid AND x.subpath = $3
              AND x.entry->>'uuid' = e.value->>'uuid')`,
        [key.projectKey, key.sessionId, key.subpath ?? "", JSON.stringify(clean)],
      );
      await c.query("COMMIT");
    } catch (e) {
      await c.query("ROLLBACK").catch(() => {});
      throw e;
    } finally {
      c.release();
    }
  }

  async load(key: SessionKey): Promise<SessionStoreEntry[] | null> {
    const { rows } = await this.pool.query<{ entry: SessionStoreEntry }>(
      `SELECT entry FROM ${T}
        WHERE project_key = $1 AND session_id = $2::uuid AND subpath = $3 ORDER BY seq`,
      [key.projectKey, key.sessionId, key.subpath ?? ""],
    );
    return rows.length === 0 ? null : rows.map((r) => r.entry);
  }

  async listSessions(projectKey: string): Promise<Array<{ sessionId: string; mtime: number }>> {
    const { rows } = await this.pool.query<{ session_id: string; mtime: string }>(
      `SELECT session_id, floor(extract(epoch FROM max(created_at)) * 1000)::bigint AS mtime
         FROM ${T} WHERE project_key = $1 AND subpath = '' GROUP BY session_id`,
      [projectKey],
    );
    return rows.map((r) => ({ sessionId: r.session_id, mtime: Number(r.mtime) }));
  }

  /** Deleting the main key cascades to every subpath of that session. */
  async delete(key: SessionKey): Promise<void> {
    if (key.subpath === undefined) {
      await this.pool.query(`DELETE FROM ${T} WHERE project_key = $1 AND session_id = $2::uuid`, [
        key.projectKey,
        key.sessionId,
      ]);
    } else {
      await this.pool.query(
        `DELETE FROM ${T} WHERE project_key = $1 AND session_id = $2::uuid AND subpath = $3`,
        [key.projectKey, key.sessionId, key.subpath],
      );
    }
  }

  async listSubkeys(key: { projectKey: string; sessionId: string }): Promise<string[]> {
    const { rows } = await this.pool.query<{ subpath: string }>(
      `SELECT DISTINCT subpath FROM ${T}
        WHERE project_key = $1 AND session_id = $2::uuid AND subpath <> ''`,
      [key.projectKey, key.sessionId],
    );
    return rows.map((r) => r.subpath);
  }

  /** True when a main transcript exists for the session under any project key. */
  async hasSession(sessionId: string): Promise<boolean> {
    const { rows } = await this.pool.query(
      `SELECT 1 FROM ${T} WHERE session_id = $1::uuid AND subpath = '' LIMIT 1`,
      [sessionId],
    );
    return rows.length > 0;
  }

  /** Removes every row of the session (all project keys, all subpaths). */
  async deleteSession(sessionId: string): Promise<void> {
    await this.pool.query(`DELETE FROM ${T} WHERE session_id = $1::uuid`, [sessionId]);
  }
}
