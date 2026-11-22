import { pathToFileURL } from "node:url";
import { assertSafeTestDatabase, createDb, type Db, insertUserEvent } from "@ci/db";
import { sql } from "drizzle-orm";
import { SECTION_KEYS, type SectionKey } from "../../src/server/coaching/types";
import { DISCLOSURE, payloadFixture } from "../../test/helpers/coaching";

// The worker's part in e2e (M5b Task 11, ADR 0074). No worker runs in the e2e stack and no AI is
// called: each step writes exactly the rows and the event the R3 jobs write for it
// (backend/crates/ai/src/jobs/report_job.rs, chat/job.rs, db/{reports,runs,chat}.rs, events.rs),
// with model text from R3's synthetic payload fixture or the step itself. Chat events are
// user-owned (`user_events.user_id` is the thread's owner, `emit_for_user`), so the stream gives
// them to that user only. Run as a script: `simulate-worker.ts '<step json>'`.

export type WorkerStep =
  | { kind: "report-section"; runId: string; section: SectionKey }
  | { kind: "chat-chunk"; messageId: string; seq: number; text: string }
  | {
      kind: "chat-done";
      messageId: string;
      replace?: boolean;
      text?: string;
      /** The validated segments (default one text segment of `text`), e.g. a cited move. */
      segments?: unknown[];
    }
  | { kind: "chat-failed"; messageId: string; reason: string }
  | { kind: "report-failed"; runId: string; reason: string }
  | { kind: "import-done"; accountId: string; imported: number };

const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";
/** The validated answer `chat-done` stores when the step gives no text and asks to replace. */
export const VALIDATED_ANSWER =
  "Most of your missed tactics come right after a capture: you recapture at once instead of checking forcing moves first.";

type Row = Record<string, unknown>;
const rows = async (db: Db, q: ReturnType<typeof sql>) => (await db.execute(q)) as unknown as Row[];

const disclosure = (promptVersion: string) => ({
  ...DISCLOSURE,
  promptVersion,
  generatedAt: new Date().toISOString(),
});

/** The step's writes and its event, in one transaction (the jobs emit last, then commit). */
export async function simulateWorker(db: Db, step: WorkerStep): Promise<void> {
  await db.transaction(async (tx) => {
    const t = tx as unknown as Db;
    if (step.kind === "report-section") await reportSection(t, step.runId, step.section);
    else if (step.kind === "report-failed") await reportFailed(t, step.runId, step.reason);
    else if (step.kind === "import-done") await importDone(t, step.accountId, step.imported);
    else await chat(t, step);
  });
}

async function reportSection(db: Db, runId: string, section: SectionKey): Promise<void> {
  const fixture = payloadFixture("example");
  const [run] = await rows(
    db,
    sql`select account_id, time_class, report_id, status from report_runs where id = ${runId} for update`,
  );
  if (!run) throw new Error(`no report run ${runId}`);
  const accountId = run.account_id as string;
  let reportId = run.report_id as string | null;
  // Step 1 (claim) and 4 (computed report): the first section of a run claims it and writes the
  // next version, computed only, with `report.section.ready {section: "computed"}`.
  if (run.status === "queued" || run.status === "awaiting_retry") {
    await db.execute(sql`update report_runs set status = 'running', started_at = now(),
      heartbeat_at = now() where id = ${runId}`);
  }
  if (!reportId) {
    const [report] = await rows(
      db,
      sql`insert into reports (account_id, time_class, version, report_run_id, schema_version,
        computed, status)
      select ${accountId}, ${run.time_class}::time_class, coalesce(max(version), 0) + 1, ${runId},
        ${fixture.schemaVersion}, ${JSON.stringify(fixture.computed)}::jsonb, 'computed_only'
      from reports where account_id = ${accountId} and time_class = ${run.time_class}::time_class
      returning id`,
    );
    reportId = report?.id as string;
    await db.execute(sql`update report_runs set report_id = ${reportId} where id = ${runId}`);
    await insertUserEvent(db, {
      accountId,
      type: "report.section.ready",
      payload: { reportId, reportRunId: runId, section: "computed", narrative: false },
    });
  }
  // Step 6: the section row, then the report's status and disclosure.
  const content = fixture.sections[section] ?? null;
  await db.execute(sql`
    insert into report_sections (report_id, section, status, content, dropped, attempts, model)
    values (${reportId}, ${section}, ${content === null ? "dropped" : "ready"}::ai_item_status,
      ${content === null ? null : JSON.stringify(content)}::jsonb, '[]'::jsonb, 1, ${DISCLOSURE.model})
    on conflict (report_id, section) do update set status = excluded.status,
      content = excluded.content, attempts = excluded.attempts, model = excluded.model,
      validated_at = now()`);
  const [{ ready } = { ready: 0 }] = await rows(
    db,
    sql`select count(*)::int as ready from report_sections where report_id = ${reportId}
      and status = 'ready' and content is not null`,
  );
  const status =
    ready === SECTION_KEYS.length ? "complete" : ready === 0 ? "computed_only" : "partial";
  await db.execute(sql`update reports set status = ${status}::report_status,
    ai_disclosure = coalesce(${status === "computed_only" ? null : JSON.stringify(disclosure("report@1"))}::jsonb, ai_disclosure),
    updated_at = now() where id = ${reportId}`);
  // `areas` is announced once per area item (`area:<id>`), every other section by its key.
  // A section's content is its envelope (`{ areas: { items } }`, Rust `ReportSectionEnvelope`).
  const items = (content as { areas?: { items?: { area?: string }[] } } | null)?.areas?.items ?? [];
  const labels =
    section === "areas" && items.length > 0
      ? [...new Set(items.map((i) => `area:${i.area}`))]
      : [section];
  for (const label of labels) {
    await insertUserEvent(db, {
      accountId,
      type: "report.section.ready",
      payload: { reportId, reportRunId: runId, section: label, narrative: content !== null },
    });
  }
  // Step 7: with every section stored, the run is done and `report.ready` follows.
  const [{ stored } = { stored: 0 }] = await rows(
    db,
    sql`select count(*)::int as stored from report_sections where report_id = ${reportId}`,
  );
  if (stored === SECTION_KEYS.length) {
    await db.execute(sql`update report_runs set status = 'done', finished_at = now(),
      next_attempt_at = null where id = ${runId}`);
    await insertUserEvent(db, {
      accountId,
      type: "report.ready",
      payload: { reportId, status },
    });
  }
}

/** `runs::set_failed` plus `job.failed {kind, reason}` (report_job.rs step 8). */
async function reportFailed(db: Db, runId: string, reason: string): Promise<void> {
  const failed = await rows(
    db,
    sql`update report_runs set status = 'failed', finished_at = now(), next_attempt_at = null,
      last_error = ${reason} where id = ${runId} and status not in ('done', 'failed')
      returning account_id`,
  );
  const accountId = failed[0]?.account_id as string | undefined;
  if (!accountId) return;
  await db.execute(sql`update jobs set state = 'failed', finished_at = now()
    where kind = 'generate-report' and payload->>'reportRunId' = ${runId}
      and state in ('queued', 'running')`);
  await insertUserEvent(db, {
    accountId,
    type: "job.failed",
    payload: { kind: "generate-report", reason },
  });
}

/**
 * An incremental import that finishes (analysis/jobs/import.rs): the due `import-account` job
 * runs, its `import_runs` row is closed as done with the count, and `import.progress` with
 * `done: true` follows. New games are not written: the e2e world keeps its games, so `imported`
 * is the count the run reports. `linked_accounts.last_synced_at` is left unset (the web never
 * reads it, and the seeds' guard treats a synced account as a real one).
 */
async function importDone(db: Db, accountId: string, imported: number): Promise<void> {
  await db.execute(sql`update jobs set state = 'done', started_at = coalesce(started_at, now()),
    finished_at = now() where kind = 'import-account' and dedupe_key = ${`import:${accountId}`}
      and state in ('queued', 'running')`);
  const [run] = await rows(
    db,
    sql`insert into import_runs (account_id, status, imported, finished_at)
      values (${accountId}, 'done', ${imported}, now()) returning id`,
  );
  await insertUserEvent(db, {
    accountId,
    type: "import.progress",
    payload: { runId: run?.id, imported, done: true },
  });
}

async function chat(
  db: Db,
  step: Extract<WorkerStep, { kind: "chat-chunk" | "chat-done" | "chat-failed" }>,
): Promise<void> {
  const [m] = await rows(
    db,
    sql`select m.status, m.turn_attempts, c.user_id, c.account_id from coach_messages m
      join coach_conversations c on c.id = m.conversation_id
      where m.id = ${step.messageId} and m.role = 'assistant' for update of m`,
  );
  if (!m) throw new Error(`no assistant message ${step.messageId}`);
  const owner = { accountId: m.account_id as string, userId: m.user_id as string };

  if (step.kind === "chat-chunk") {
    // The job reserves an attempt as it starts streaming (chat/job.rs); chunks carry it.
    const [turn] = await rows(
      db,
      sql`update coach_messages set status = 'streaming',
        streaming_at = case when status = 'pending' then now() else streaming_at end,
        turn_attempts = turn_attempts + case when status = 'pending' then 1 else 0 end
      where id = ${step.messageId} and status in ('pending', 'streaming')
      returning turn_attempts`,
    );
    const attempt = Number(turn?.turn_attempts ?? m.turn_attempts ?? 0);
    await insertUserEvent(db, {
      ...owner,
      type: "chat.chunk",
      payload: {
        messageId: step.messageId,
        seq: step.seq,
        text: step.text,
        segments: [{ kind: "text", text: step.text }],
        citations: [],
        interim: false,
        attempt,
      },
    });
    return;
  }

  if (step.kind === "chat-failed") {
    const failed = await rows(
      db,
      sql`update coach_messages set status = 'failed', failure_reason = ${step.reason}
        where id = ${step.messageId} and status in ('pending', 'streaming') returning id`,
    );
    if (failed.length === 1) {
      await insertUserEvent(db, {
        ...owner,
        type: "chat.message.failed",
        payload: {
          messageId: step.messageId,
          reason: step.reason,
          attempt: Number(m.turn_attempts),
        },
      });
    }
    return;
  }

  // chat-done: the validated text (the streamed chunks joined, unless replaced), its segments
  // and the disclosure; `replace` says the validated text differs from what streamed.
  let text = step.text;
  if (text === undefined && step.replace) text = VALIDATED_ANSWER;
  if (text === undefined) {
    const chunks = await rows(
      db,
      sql`select payload->>'text' as text from user_events where type = 'chat.chunk'
        and payload->>'messageId' = ${step.messageId} order by (payload->>'seq')::int`,
    );
    text = chunks.map((c) => c.text as string).join("");
  }
  const done = await rows(
    db,
    sql`update coach_messages set status = 'done', text = ${text},
      segments = ${JSON.stringify(step.segments ?? [{ kind: "text", text }])}::jsonb, citations = '[]'::jsonb,
      dropped_count = 0, ai_disclosure = ${JSON.stringify(disclosure("chat@1"))}::jsonb,
      failure_reason = null
      where id = ${step.messageId} and status in ('pending', 'streaming') returning id`,
  );
  if (done.length === 1) {
    await insertUserEvent(db, {
      ...owner,
      type: "chat.message.ready",
      payload: {
        messageId: step.messageId,
        replace: step.replace === true,
        attempt: Number(m.turn_attempts),
      },
    });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const raw = process.argv[2];
  if (!raw) throw new Error("usage: simulate-worker.ts '<step json>' | latest-run | latest-answer");
  const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
  if (raw === "latest-run") {
    // The run "Rebuild report" just inserted (the spec needs its id, as the worker gets it).
    const [run] = await rows(db, sql`select id from report_runs order by created_at desc limit 1`);
    console.log(`RUN=${run?.id ?? ""}`);
  } else if (raw === "latest-answer") {
    // The answer the web created for the question just sent (status pending).
    const [m] = await rows(
      db,
      sql`select id from coach_messages where role = 'assistant'
        and status in ('pending', 'streaming') order by created_at desc limit 1`,
    );
    console.log(`MESSAGE=${m?.id ?? ""}`);
  } else {
    await simulateWorker(db, JSON.parse(raw) as WorkerStep);
    console.log("SIMULATED");
  }
  process.exit(0);
}
