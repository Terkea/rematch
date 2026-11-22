import { readFileSync } from "node:fs";
import type { Db } from "@ci/db";
import {
  reportRuns,
  reportSections,
  reports,
  weaknessChangeNotes,
  weeklyReviews,
} from "@ci/db/generated";
import { sql } from "drizzle-orm";
import type {
  AiDisclosure,
  AiItemStatus,
  NarrativeItem,
  ReportPayload,
  ReportRunRow,
  SectionKey,
  WeeklyContent,
} from "../../src/server/coaching/types";
import type { TimeClass } from "../../src/server/games/types";
import { assertSeedable } from "./games";

// Test-only writers for the coaching read model. They write R3's synthetic payload fixtures
// (`backend/fixtures/ai/payload`), never owner data, and each runs `assertSeedable` first.

const FIXTURES = new URL("../../../../backend/fixtures/ai/payload/", import.meta.url);

/** A synthetic R3 payload snapshot: `example` (complete) or `partial`. */
export function payloadFixture(which: "example" | "partial"): ReportPayload {
  const file =
    which === "example" ? "report-payload.example.json" : "report-payload.partial.example.json";
  return JSON.parse(readFileSync(new URL(file, FIXTURES), "utf8")) as ReportPayload;
}

export const DISCLOSURE: AiDisclosure = {
  label: "Written by AI",
  note: "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.",
  model: "claude-sonnet-5-5",
  promptVersion: "report@1",
  generatedAt: "2026-11-20T08:30:00.000Z",
};

/** Empties the coaching tables (runs, reports, sections, weekly reviews, notes and jobs). */
export async function resetCoachingTables(db: Db): Promise<void> {
  await assertSeedable(db);
  await db.execute(
    sql`truncate table report_runs, reports, report_sections, weekly_reviews, weakness_change_notes, jobs cascade`,
  );
}

/**
 * A report run and, with `payload`, its report: `reports.computed` and the `report_sections`
 * of the fixture (a null section is stored `dropped`, its fixture `dropped` items with a
 * synthetic `detail`). The run defaults to `done` with a payload and `queued` without one.
 */
export async function seedReport(
  db: Db,
  accountId: string,
  opts: {
    timeClass: TimeClass;
    payload?: "example" | "partial";
    run?: Partial<ReportRunRow>;
    createdAt?: Date;
  },
): Promise<{ runId: string; reportId: string | null }> {
  await assertSeedable(db);
  const createdAt = opts.createdAt ?? new Date();
  const [run] = await db
    .insert(reportRuns)
    .values({
      accountId,
      timeClass: opts.timeClass,
      trigger: "onboarding",
      plan: "free",
      mode: "sync",
      status: opts.payload ? "done" : "queued",
      promptVersion: "report@1",
      isFirstReport: true,
      createdAt,
      ...opts.run,
    })
    .returning({ id: reportRuns.id });
  if (!run) throw new Error("seedReport: no run");
  if (!opts.payload) return { runId: run.id, reportId: null };

  const fixture = payloadFixture(opts.payload);
  const [{ next } = { next: 1 }] = (await db.execute(sql`
    select coalesce(max(version), 0)::int + 1 as next from reports
    where account_id = ${accountId} and time_class = ${opts.timeClass}::time_class
  `)) as unknown as { next: number }[];
  const [report] = await db
    .insert(reports)
    .values({
      accountId,
      timeClass: opts.timeClass,
      version: next,
      reportRunId: run.id,
      schemaVersion: fixture.schemaVersion,
      computed: fixture.computed,
      status: fixture.status,
      aiDisclosure: fixture.aiDisclosure,
      createdAt,
      updatedAt: createdAt,
    })
    .returning({ id: reports.id });
  if (!report) throw new Error("seedReport: no report");
  const entries = Object.entries(fixture.sections) as [SectionKey, unknown][];
  if (entries.length > 0) {
    await db.insert(reportSections).values(
      entries.map(([section, content]) => ({
        reportId: report.id,
        section,
        status: content === null ? ("dropped" as const) : ("ready" as const),
        content,
        dropped: fixture.dropped
          .filter((d) => d.section === section)
          .map((d) => ({
            ...d,
            errors: d.errors.map((e) => ({ ...e, detail: "synthetic rejected fragment" })),
          })),
        attempts: 1,
        model: DISCLOSURE.model,
      })),
    );
  }
  await db.execute(sql`update report_runs set report_id = ${report.id} where id = ${run.id}`);
  return { runId: run.id, reportId: report.id };
}

const item = (text: string): NarrativeItem => ({
  text,
  segments: [{ kind: "text", text }],
  citations: [],
});

/** A synthetic validated weekly review. */
export const WEEKLY_CONTENT: WeeklyContent = {
  headline: item("A steadier week."),
  process: item("You took more time on critical moves."),
  outcomes: [{ ...item("Fewer hung pieces."), weaknessId: "W1", label: "Hung pieces" }],
  exemplars: [],
  nextWeek: item("Keep checking captures before you move."),
};

export async function seedWeekly(
  db: Db,
  accountId: string,
  opts: { timeClass: TimeClass; weekStart: string; status?: AiItemStatus; dropped?: number },
): Promise<{ id: string }> {
  await assertSeedable(db);
  const status = opts.status ?? "ready";
  const ready = status === "ready";
  const [row] = await db
    .insert(weeklyReviews)
    .values({
      accountId,
      timeClass: opts.timeClass,
      weekStart: opts.weekStart,
      status,
      input: { synthetic: true },
      content: ready ? WEEKLY_CONTENT : null,
      dropped: Array.from({ length: opts.dropped ?? 0 }, (_, i) => ({
        section: "weekly",
        path: `outcomes[${i}]`,
        errors: [],
      })),
      model: ready ? DISCLOSURE.model : null,
      aiDisclosure: ready ? { ...DISCLOSURE, promptVersion: "weekly@1" } : null,
    })
    .returning({ id: weeklyReviews.id });
  if (!row) throw new Error("seedWeekly: no row");
  return { id: row.id };
}

export const NOTE_ITEM: NarrativeItem = item("This rose after you switched to a new opening.");

export async function seedNote(
  db: Db,
  accountId: string,
  opts: { changeKey: string; seriesKey: string; onsetDate?: string | null; status?: AiItemStatus },
): Promise<{ id: string }> {
  await assertSeedable(db);
  const status = opts.status ?? "ready";
  const ready = status === "ready";
  const [row] = await db
    .insert(weaknessChangeNotes)
    .values({
      accountId,
      changeKey: opts.changeKey,
      seriesKey: opts.seriesKey,
      onsetDate: opts.onsetDate === undefined ? "2026-10-02" : opts.onsetDate,
      status,
      input: { synthetic: true },
      text: ready ? NOTE_ITEM.text : null,
      segments: ready ? NOTE_ITEM.segments : null,
      citations: ready ? NOTE_ITEM.citations : [],
      model: ready ? DISCLOSURE.model : null,
      aiDisclosure: ready ? { ...DISCLOSURE, promptVersion: "change@1" } : null,
    })
    .returning({ id: weaknessChangeNotes.id });
  if (!row) throw new Error("seedNote: no row");
  return { id: row.id };
}
