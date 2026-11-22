import type { Db } from "@ci/db";
import { aiStatus } from "@ci/db/generated";
import { sql } from "drizzle-orm";

/**
 * Whether the coach can write right now (bug hunt I5, ADR 0078), from the `ai_status` rows the
 * worker keeps fresh:
 * - `available`: a running worker has an AI client;
 * - `no_ai`: workers run, none with an AI client (no sidecar, no credential, a failed health
 *   check), so a report is computed-only and a question would not be answered;
 * - `offline`: no worker has a fresh row, so nothing would pick a request up.
 */
export type CoachStatus = "available" | "no_ai" | "offline";

/** Rust `worker::ai_status::STALE_AFTER`: four missed 30 s heartbeats. */
export const AI_STATUS_STALE_SECONDS = 120;

export async function coachStatus(db: Db): Promise<CoachStatus> {
  const [row] = await db
    .select({ enabled: sql<boolean>`bool_or(${aiStatus.enabled})` })
    .from(aiStatus)
    // The database's clock on both sides, so no web/worker clock skew matters.
    .where(
      sql`${aiStatus.heartbeatAt} > now() - make_interval(secs => ${AI_STATUS_STALE_SECONDS})`,
    );
  const enabled = row?.enabled ?? null;
  if (enabled === null) return "offline";
  return enabled ? "available" : "no_ai";
}

export { COACH_UNAVAILABLE_TITLE } from "./status-copy";
