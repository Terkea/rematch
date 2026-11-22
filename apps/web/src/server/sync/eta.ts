import type { Db } from "@ci/db";
import { SQL, sql } from "drizzle-orm";
import { ANALYSE_JOB } from "./jobs";

/**
 * The analysis ETA, mirrored from the scheduler (`backend/crates/scheduler/src/eta.rs`,
 * R2 Task 30, ADR-R2-17) so the web says exactly what `cli eta` says. It is `null` unless there
 * is a measured throughput (at least `minRateRows` finished analyses in the last 10 minutes, from
 * `job_metrics`) and every queued analyse job of the account has an exact place in the fair order
 * (the first `window` places). Never a guess, never NaN or infinite. The SQL is the Rust text,
 * byte for byte (`test/sync/contract.test.ts`).
 */
export const ETA = {
  minRateRows: 5,
  rateWindowSecs: 600,
  window: 400,
  /** `fair::DEFAULT_SLICE` and `fair::DEFAULT_SERVED_WINDOW`. */
  slice: 5,
  servedWindowMs: 3_600_000,
} as const;

/** `position_sql()` after `{fair}`: the greatest place of the account's jobs, and how many placed. */
export const POSITION_TAIL =
  ", ordered AS (SELECT user_key, row_number() OVER (ORDER BY tier, slice, joined_at, priority, run_at, id) AS pos FROM (SELECT * FROM fair ORDER BY tier, slice, joined_at, priority, run_at, id LIMIT $2) f) SELECT coalesce(max(pos), 0)::int8, count(*)::int8 FROM ordered WHERE user_key = $5";

export type Eta = {
  /** Seconds until the account's last queued analysis should finish. */
  seconds: number;
  /** Finished analyses (all accounts) in the last 10 minutes: what the pace is measured on. */
  basis: number;
};

/** Binds Rust-style `$n` placeholders as query parameters (an `SQL` value is inlined as SQL). */
function positional(text: string, params: Record<number, unknown>): SQL {
  const parts = text.split(/\$(\d+)/);
  const chunks: SQL[] = [];
  parts.forEach((part, i) => {
    if (i % 2 === 0) {
      chunks.push(sql.raw(part));
      return;
    }
    const value = params[Number(part)];
    chunks.push(value instanceof SQL ? value : sql`${value}`);
  });
  return sql.join(chunks, sql.raw(""));
}

const int = (v: unknown) => Number(v ?? 0);

/** The account's ETA (see the module comment), read on `tx`. */
export async function accountEta(tx: Db, accountId: string): Promise<Eta | null> {
  const [rate] = (await tx.execute(sql`
    SELECT count(*)::int8 AS n FROM job_metrics
    WHERE kind = ${ANALYSE_JOB.kind} AND created_at > now() - interval '10 minutes'
  `)) as unknown as { n: string }[];
  const rows = int(rate?.n);
  if (rows < ETA.minRateRows) return null;
  const perSec = rows / ETA.rateWindowSecs;

  const [queued] = (await tx.execute(sql`
    SELECT count(*)::int8 AS n FROM jobs
    WHERE state = 'queued' AND kind = ${ANALYSE_JOB.kind} AND user_key = ${accountId}::uuid
  `)) as unknown as { n: string }[];
  const waiting = int(queued?.n);
  // Nothing queued: no finish time to state (Rust gives Some(0) only when all is done).
  if (waiting === 0) return null;

  const [pos] = (await tx.execute(
    positional(FAIR_ORDER_SQL + POSITION_TAIL, {
      1: sql.raw(`ARRAY['${ANALYSE_JOB.kind}']`),
      2: ETA.window,
      3: ETA.servedWindowMs,
      4: ETA.slice,
      5: accountId,
    }),
  )) as unknown as { coalesce: string; count: string }[];
  const last = int(pos?.coalesce);
  const found = int(pos?.count);
  if (found !== waiting || !(perSec > 0)) return null;
  const seconds = last / perSec;
  if (!Number.isFinite(seconds)) return null;
  return { seconds: Math.round(seconds), basis: rows };
}

/** `scheduler::fair::FAIR_ORDER_SQL`, byte for byte: the queued jobs in claim order. */
export const FAIR_ORDER_SQL = `WITH RECURSIVE
kinds AS (SELECT DISTINCT unnest($1::text[]) AS kind),
users AS (
  SELECT k.kind,
    (SELECT j.user_key FROM jobs j
     WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NOT NULL
     ORDER BY j.user_key LIMIT 1) AS user_key
  FROM kinds k
  UNION ALL
  SELECT u.kind,
    (SELECT j.user_key FROM jobs j
     WHERE j.state = 'queued' AND j.kind = u.kind AND j.user_key > u.user_key
     ORDER BY j.user_key LIMIT 1)
  FROM users u WHERE u.user_key IS NOT NULL),
kind_heads AS (
  SELECT u.user_key, c.id, c.priority, c.run_at
  FROM users u CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = u.kind AND j.user_key = u.user_key
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT 1) c
  WHERE u.user_key IS NOT NULL
  UNION ALL
  SELECT NULL::uuid, c.id, c.priority, c.run_at
  FROM kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT 1) c),
heads AS (
  SELECT DISTINCT ON (user_key) user_key, id, priority, run_at FROM kind_heads
  ORDER BY user_key, priority, run_at, id),
served AS (
  SELECT g.account_id AS user_key, count(*) AS n FROM games g
  WHERE g.analysis_started_at > now() - $3::float8 * interval '1 millisecond'
  GROUP BY g.account_id),
head_keys AS (
  SELECT h.user_key, coalesce(s.n, 0) AS served,
    coalesce((SELECT a.created_at FROM linked_accounts a WHERE a.id = h.user_key),
             'infinity'::timestamptz) AS joined_at,
    h.priority / 2 AS tier, coalesce(s.n, 0) / $4::int8 AS slice, h.priority, h.run_at, h.id
  FROM heads h LEFT JOIN served s ON s.user_key = h.user_key),
chosen AS (
  SELECT user_key, served, joined_at FROM head_keys
  ORDER BY tier, slice, joined_at, priority, run_at, id
  LIMIT $2),
ready_rows AS (
  SELECT c.id, ch.user_key, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key = ch.user_key
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT $2) c
  WHERE ch.user_key IS NOT NULL
  UNION ALL
  SELECT c.id, NULL::uuid, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT $2) c
  WHERE ch.user_key IS NULL),
fair AS (
  SELECT r.id, r.user_key, r.priority / 2 AS tier,
    (r.served + row_number() OVER (PARTITION BY r.user_key
                                   ORDER BY r.priority, r.run_at, r.id) - 1) / $4::int8 AS slice,
    r.joined_at, r.priority, r.run_at
  FROM ready_rows r)`;
