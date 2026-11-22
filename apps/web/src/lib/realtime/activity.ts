import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "../query-keys";

/**
 * Background work the header reports (M4 T23): unfinished imports per linked account, and job
 * failures since the page loaded. It lives only in the query cache under `queryKeys.activity()`;
 * the stream writes it (`applyEvent`) and `ActivityIndicator` reads it.
 */
export type Activity = {
  imports: Record<string, { imported: number; total?: number }>;
  failures: { kind: string; reason: string; at: number }[];
};

export const EMPTY_ACTIVITY: Activity = Object.freeze({
  imports: Object.freeze({}),
  failures: Object.freeze([]) as unknown as Activity["failures"],
}) as Activity;

/** The most failures kept; older ones drop off the front. */
export const MAX_FAILURES = 20;

export function readActivity(qc: QueryClient): Activity {
  return qc.getQueryData<Activity>(queryKeys.activity()) ?? EMPTY_ACTIVITY;
}

export function updateActivity(qc: QueryClient, update: (a: Activity) => Activity): void {
  qc.setQueryData<Activity>(queryKeys.activity(), (old) => update(old ?? EMPTY_ACTIVITY));
}

/** All unfinished imports as one count; the total only when every import knows its own. */
export function importSummary(a: Activity): { done: number; total?: number } | null {
  const runs = Object.values(a.imports);
  if (runs.length === 0) return null;
  const done = runs.reduce((sum, r) => sum + r.imported, 0);
  if (runs.some((r) => r.total === undefined)) return { done };
  return { done, total: runs.reduce((sum, r) => sum + (r.total ?? 0), 0) };
}

/** The worker's job kinds (Rust `queue::kinds`) in words; anything else reads as sent. */
const KIND_LABELS: Record<string, string> = {
  "import-account": "games import",
  "analyse-game": "game analysis",
  "analyse-reference-game": "reference game analysis",
  "preview-game": "game preview",
  "schedule-analysis": "analysis scheduling",
  "build-profile": "profile build",
  reconcile: "sync check",
};

export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind;
}

/** Import kinds: a failure of one ends that account's import. */
export function isImportKind(kind: string): boolean {
  return kind === "import-account" || kind === "import";
}
