import { randomUUID } from "node:crypto";
import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { assertSeedable } from "./games";

// Test-only writers for the sync and analysis-progress read model: synthetic games by status,
// jobs, import runs and job metrics. Each runs `assertSeedable` first.

export type SeedStatus = "imported" | "pending" | "analysing" | "done" | "failed" | "skipped";

/** Empties the queue tables the sync read model reads (games and runs go with `resetGameTables`). */
export async function resetSyncTables(db: Db): Promise<void> {
  await assertSeedable(db);
  await db.execute(sql`truncate table jobs, job_metrics, import_runs`);
}

/** `n` synthetic games of the account in `status`; returns their ids. */
export async function seedGames(
  db: Db,
  accountId: string,
  status: SeedStatus,
  n: number,
): Promise<string[]> {
  await assertSeedable(db);
  const ids: string[] = [];
  for (let i = 0; i < n; i++) {
    const rows = (await db.execute(sql`
      insert into games (account_id, platform, platform_game_id, pgn, played_at, time_class, rated,
        player_color, white_name, black_name, result, analysis_status, selected_at)
      select a.id, a.platform, ${randomUUID()}, '1. e4 e5 *', now() - ${i} * interval '1 hour',
        'rapid', true, 'white', 'me', 'opp', '*', ${status}::analysis_status,
        case when ${status} = 'imported' then null else now() end
      from linked_accounts a where a.id = ${accountId}
      returning id::text
    `)) as unknown as { id: string }[];
    const id = rows[0]?.id;
    if (!id) throw new Error("seedGames: no row");
    ids.push(id);
  }
  return ids;
}

/** One job row. `createdAt`/`runAt` default to now. */
export async function seedJob(
  db: Db,
  job: {
    kind: string;
    dedupeKey: string;
    userKey: string;
    state?: "queued" | "running" | "done" | "failed" | "dead" | "paused";
    createdAt?: Date;
    runAt?: Date;
    payload?: unknown;
    priority?: number;
  },
): Promise<number> {
  await assertSeedable(db);
  const created = job.createdAt?.toISOString() ?? null;
  const runAt = job.runAt?.toISOString() ?? null;
  const rows = (await db.execute(sql`
    insert into jobs (kind, payload, dedupe_key, user_key, state, priority, created_at, run_at)
    values (${job.kind}, ${JSON.stringify(job.payload ?? {})}::jsonb, ${job.dedupeKey},
      ${job.userKey}::uuid, ${job.state ?? "queued"}::job_state, ${job.priority ?? 100}::int2,
      coalesce(${created}::timestamptz, now()), coalesce(${runAt}::timestamptz, now()))
    returning id::text
  `)) as unknown as { id: string }[];
  return Number(rows[0]?.id);
}

/** One import run. */
export async function seedImportRun(
  db: Db,
  accountId: string,
  run: {
    status: "running" | "done" | "failed" | "paused";
    imported?: number;
    startedAt: Date;
    finishedAt?: Date | null;
    heartbeatAt?: Date;
  },
): Promise<void> {
  await assertSeedable(db);
  await db.execute(sql`
    insert into import_runs (account_id, status, imported, started_at, heartbeat_at, finished_at)
    values (${accountId}, ${run.status}::import_run_status, ${run.imported ?? 0},
      ${run.startedAt.toISOString()}::timestamptz,
      ${(run.heartbeatAt ?? run.startedAt).toISOString()}::timestamptz,
      ${run.finishedAt ? run.finishedAt.toISOString() : null}::timestamptz)
  `);
}

/** `n` finished analyse-game metrics rows, `agoMs` before now. */
export async function seedAnalyseMetrics(db: Db, n: number, agoMs = 60_000): Promise<void> {
  await assertSeedable(db);
  for (let i = 0; i < n; i++) {
    await db.execute(sql`
      insert into job_metrics (kind, total_ms, created_at)
      values ('analyse-game', 1000, now() - ${agoMs} * interval '1 millisecond')
    `);
  }
}
