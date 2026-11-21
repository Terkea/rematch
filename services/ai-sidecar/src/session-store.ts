import type { SessionKey, SessionStoreEntry } from "@anthropic-ai/claude-agent-sdk";
import type pg from "pg";
import type { ChatSessionStore } from "./chat.js";

/**
 * The Agent SDK `SessionStore` on Postgres (ADR-R3-6). It touches one table,
 * `ai_sdk_session_entries` (its migration is owned by Task 17), as the `ai_sidecar` role, which may only
 * SELECT, INSERT and DELETE there.
 *
 * Entries are opaque: they go to Postgres as one JSON string and come back as jsonb values, and the
 * sidecar never inspects them. Only the partial unique index on the SDK-documented top-level `uuid`
 * reads into them, as the idempotency key the docs ask adapters to use for retried batches.
 *
 * Rust owns deletion of a conversation: it deletes this table's rows for the conversation id in the
 * same transaction as the conversation (Tasks 17, 25). The sidecar deletes only to clear a
 * transcript it could not resume, before rebuilding it.
 */
const T = "ai_sdk_session_entries";
/** Advisory-lock class of the per-conversation turn lock: (TURN_LOCK_CLASS, hashtext(id)). */
export const TURN_LOCK_CLASS = 2026112110;

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
  /** Called when a held turn-lock connection drops; exclusion is gone for the rest of that turn. */
  onLockLost?: () => void;
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
        // Retried batches re-deliver entries: the partial unique index on the entry uuid (Task 17)
        // turns a repeat into a no-op, within a batch as well as across batches.
        `INSERT INTO ${T} (project_key, session_id, subpath, seq, entry)
         SELECT $1, $2::uuid, $3, base.m + e.ord, e.value
         FROM jsonb_array_elements($4::jsonb) WITH ORDINALITY AS e(value, ord),
              (SELECT coalesce(max(seq), 0) AS m FROM ${T}
                WHERE project_key = $1 AND session_id = $2::uuid AND subpath = $3) AS base
         ON CONFLICT DO NOTHING`,
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

  /**
   * Takes the cross-replica turn lock for a conversation: a session-level advisory lock held on a
   * dedicated pool connection for the whole turn. Returns its release function, or null when
   * another holder (another replica) has it. The key is the two-int form
   * (TURN_LOCK_CLASS, hashtext(conversation id)), so it never collides with the single-bigint
   * per-transcript append locks.
   */
  async tryLockConversation(sessionId: string): Promise<(() => Promise<void>) | null> {
    const c = await this.pool.connect();
    // A checked-out pg client with no 'error' listener crashes the process when its connection
    // drops (e.g. pg_terminate_backend, 57P01). The listener stays for the lock's whole lifetime.
    let lost = false;
    const onError = () => {
      if (lost) return;
      lost = true;
      // The server already dropped the session lock with the connection: exclusion is gone for
      // the rest of this turn. The turn itself carries on.
      this.opts.onLockLost?.();
    };
    c.on("error", onError);
    const done = (destroy: boolean) => {
      c.off("error", onError);
      c.release(destroy);
    };
    try {
      // Server-side keepalives on this long-held connection, so a partitioned lock holder is
      // detected and its lock dropped within about two minutes (60 s + 3 x 10 s, plus slack).
      await c.query(
        "SET tcp_keepalives_idle = 60; SET tcp_keepalives_interval = 10; SET tcp_keepalives_count = 3",
      );
      const { rows } = await c.query<{ ok: boolean }>(
        "SELECT pg_try_advisory_lock($1::int, hashtext($2)) AS ok",
        [TURN_LOCK_CLASS, sessionId],
      );
      if (!rows[0]?.ok) {
        await c.query(
          "RESET tcp_keepalives_idle; RESET tcp_keepalives_interval; RESET tcp_keepalives_count",
        );
        done(false);
        return null;
      }
    } catch (e) {
      done(true);
      throw e;
    }
    let released = false;
    return async () => {
      if (released) return;
      released = true;
      if (lost) {
        done(true); // the connection is dead; destroy it rather than return it to the pool
        return;
      }
      try {
        await c.query("SELECT pg_advisory_unlock($1::int, hashtext($2))", [
          TURN_LOCK_CLASS,
          sessionId,
        ]);
        await c.query(
          "RESET tcp_keepalives_idle; RESET tcp_keepalives_interval; RESET tcp_keepalives_count",
        );
        done(false);
      } catch {
        done(true); // destroying the connection drops its session locks
      }
    };
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
