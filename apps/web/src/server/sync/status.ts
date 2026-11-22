import type { Db } from "@ci/db";
import { type SQL, sql } from "drizzle-orm";
import { readOnly } from "../games/read-only";
import { accountEta, type Eta } from "./eta";
import { ANALYSE_JOB, IMPORT_JOB, PROFILE_KIND, SYNC_COOLDOWN_MS } from "./jobs";

/**
 * Where an account's import stands:
 * - `checking`: its import job is due (R1 `due!()`) or running;
 * - `waiting`: a queued import the worker deliberately held back (a retry backoff, an open
 *   breaker, `account_busy`); it runs at `retryAt`, and "Sync now" never moves it;
 * - `paused`: the job is paused (it resumes by itself);
 * - `idle`: no live import.
 */
export type SyncPhase = "idle" | "checking" | "waiting" | "paused";

/** Games by `games.analysis_status`. A failed game with a live retry job is `retrying`, not `failed`. */
export type AnalysisCounts = {
  waiting: number;
  analysing: number;
  done: number;
  failed: number;
  retrying: number;
  /** `imported`: stored but not picked for analysis by the scheduler (yet). */
  notSelected: number;
  skipped: number;
};

export type AccountSync = {
  accountId: string;
  platform: "chesscom" | "lichess";
  name: string;
  /** The user's own link to it is verified. */
  verified: boolean;
  phase: SyncPhase;
  /** When a `waiting` import runs (its `run_at`); null in any other phase. */
  retryAt: string | null;
  /** The newest finished import run: when it ended and how many new games it stored. */
  lastCheck: { at: string; found: number } | null;
  /** The newest run failed (after `lastCheck`, if any). */
  lastRunFailed: boolean;
  /** When "Sync now" is allowed again; null when it is allowed now. */
  nextManualAt: string | null;
  analysis: AnalysisCounts;
  eta: Eta | null;
  /** A build-profile job of the account is queued or running. */
  profileUpdating: boolean;
};

export type SyncStatus = { generatedAt: string; accounts: AccountSync[] };

/** R1's `due!()` for a job row `j`: claimable now, or enqueued to run at once. */
export const DUE = sql.raw("(j.run_at <= now() OR j.run_at <= j.created_at)");

/**
 * When "Sync now" is allowed again for the account, in database time: 2 minutes after the
 * latest import run start or due import job (so a sync the worker started counts too). Null
 * when it has never run. Compare it with the database's `now()`, never the web host's clock.
 */
export function availableAtSql(accountId: SQL): SQL {
  return sql`(greatest(
    (SELECT max(r.started_at) FROM import_runs r WHERE r.account_id = ${accountId}),
    (SELECT max(j.created_at) FROM jobs j
      WHERE j.kind = ${IMPORT_JOB.kind} AND j.user_key = ${accountId} AND ${DUE}))
    + ${SYNC_COOLDOWN_MS} * interval '1 millisecond')`;
}

type Row = {
  account_id: string;
  platform: "chesscom" | "lichess";
  name: string;
  verified: boolean;
  live_state: string | null;
  live_due: boolean | null;
  live_run_at: Date | string | null;
  good_at: Date | string | null;
  good_found: number | null;
  newest_status: string | null;
  next_manual_at: Date | string | null;
  profile_updating: boolean;
  waiting: string;
  analysing: string;
  done: string;
  failed: string;
  retrying: string;
  not_selected: string;
  skipped: string;
};

const iso = (v: Date | string) => new Date(v).toISOString();

/**
 * Sync and analysis progress for each of the user's linked accounts (every link, verified or
 * not, like the games they read), in one read-only snapshot.
 */
export async function getSyncStatus(db: Db, userId: string): Promise<SyncStatus> {
  const now = new Date();
  return readOnly(db, async (tx) => {
    const rows = (await tx.execute(sql`
      SELECT a.id::text AS account_id, a.platform::text AS platform, a.display_name AS name,
        EXISTS (SELECT 1 FROM account_links v WHERE v.user_id = ${userId}
          AND v.linked_account_id = a.id AND v.verified_at IS NOT NULL) AS verified,
        live.state AS live_state, live.due AS live_due, live.run_at AS live_run_at,
        good.finished_at AS good_at, good.imported AS good_found,
        newest.status AS newest_status,
        CASE WHEN avail.at > now() THEN avail.at END AS next_manual_at,
        EXISTS (SELECT 1 FROM jobs p WHERE p.kind = ${PROFILE_KIND} AND p.user_key = a.id
          AND p.state IN ('queued', 'running')) AS profile_updating,
        counts.*
      FROM linked_accounts a
      LEFT JOIN LATERAL (
        SELECT j.state::text AS state, ${DUE} AS due, j.run_at FROM jobs j
        WHERE j.kind = ${IMPORT_JOB.kind} AND j.dedupe_key = ${IMPORT_JOB.key("")} || a.id::text
          AND j.state IN ('queued', 'running', 'paused')
        ORDER BY j.id DESC LIMIT 1) live ON true
      CROSS JOIN LATERAL (SELECT ${availableAtSql(sql`a.id`)} AS at) avail
      LEFT JOIN LATERAL (
        SELECT r.finished_at, r.imported FROM import_runs r
        WHERE r.account_id = a.id AND r.status = 'done' AND r.finished_at IS NOT NULL
        ORDER BY r.finished_at DESC, r.id DESC LIMIT 1) good ON true
      LEFT JOIN LATERAL (
        SELECT r.status::text AS status FROM import_runs r
        WHERE r.account_id = a.id ORDER BY r.started_at DESC, r.id DESC LIMIT 1) newest ON true
      CROSS JOIN LATERAL (
        SELECT
          count(*) FILTER (WHERE g.analysis_status = 'pending')::int8 AS waiting,
          count(*) FILTER (WHERE g.analysis_status = 'analysing')::int8 AS analysing,
          count(*) FILTER (WHERE g.analysis_status = 'done')::int8 AS done,
          count(*) FILTER (WHERE g.analysis_status = 'failed' AND NOT g.retry_live)::int8 AS failed,
          count(*) FILTER (WHERE g.analysis_status = 'failed' AND g.retry_live)::int8 AS retrying,
          count(*) FILTER (WHERE g.analysis_status = 'imported')::int8 AS not_selected,
          count(*) FILTER (WHERE g.analysis_status = 'skipped')::int8 AS skipped
        FROM (
          SELECT g.analysis_status,
            g.analysis_status = 'failed' AND EXISTS (
              SELECT 1 FROM jobs j WHERE j.kind = ${ANALYSE_JOB.kind}
                AND j.dedupe_key = ${ANALYSE_JOB.key("")} || g.id::text
                AND j.state IN ('queued', 'running', 'paused')) AS retry_live
          FROM games g WHERE g.account_id = a.id) g) counts
      WHERE a.id IN (SELECT l.linked_account_id FROM account_links l WHERE l.user_id = ${userId})
      ORDER BY a.platform, a.display_name, a.id
    `)) as unknown as Row[];

    const accounts: AccountSync[] = [];
    for (const r of rows) {
      const analysis: AnalysisCounts = {
        waiting: Number(r.waiting),
        analysing: Number(r.analysing),
        done: Number(r.done),
        failed: Number(r.failed),
        retrying: Number(r.retrying),
        notSelected: Number(r.not_selected),
        skipped: Number(r.skipped),
      };
      const phase: SyncPhase =
        r.live_state === "paused"
          ? "paused"
          : r.live_state === "running" || (r.live_state === "queued" && r.live_due)
            ? "checking"
            : r.live_state === "queued"
              ? "waiting"
              : "idle";
      const outstanding = analysis.waiting + analysis.analysing + analysis.retrying > 0;
      accounts.push({
        accountId: r.account_id,
        platform: r.platform,
        name: r.name,
        verified: r.verified,
        phase,
        retryAt: phase === "waiting" && r.live_run_at ? iso(r.live_run_at) : null,
        lastCheck: r.good_at ? { at: iso(r.good_at), found: Number(r.good_found ?? 0) } : null,
        lastRunFailed: r.newest_status === "failed",
        nextManualAt: r.next_manual_at ? iso(r.next_manual_at) : null,
        analysis,
        eta: outstanding ? await accountEta(tx, r.account_id) : null,
        profileUpdating: r.profile_updating,
      });
    }
    return { generatedAt: now.toISOString(), accounts };
  });
}
