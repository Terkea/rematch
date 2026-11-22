import type { StreamEvent } from "@ci/db";
import { toast } from "@ci/ui/overlays";
import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "../query-keys";
import { isImportKind, kindLabel, MAX_FAILURES, updateActivity } from "./activity";

export type { StreamEvent };

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const isCount = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0;

const article = (word: string) => (/^[aeiou]/i.test(word) ? "An" : "A");

/**
 * Applies one stream event to the query cache (M4 T23). Each type touches only its own keys;
 * `reset` (the stream's cursor predates pruned events) is the one invalidation of everything.
 * Unknown types and malformed payloads are ignored, so a newer worker never breaks the page.
 */
export function applyEvent(qc: QueryClient, e: StreamEvent): void {
  const p = e.payload;
  switch (e.type) {
    case "import.progress": {
      if (!isRecord(p) || !isCount(p.imported)) return;
      qc.setQueryData(queryKeys.importProgress(e.accountId), p);
      const imported = p.imported;
      const total = isCount(p.total) ? p.total : undefined;
      const finished = p.done === true || (total !== undefined && imported >= total);
      updateActivity(qc, (a) => {
        const { [e.accountId]: _, ...others } = a.imports;
        if (finished) return { ...a, imports: others };
        const run = total === undefined ? { imported } : { imported, total };
        return { ...a, imports: { ...others, [e.accountId]: run } };
      });
      return;
    }
    case "game.analysed": {
      if (!isRecord(p) || typeof p.gameId !== "string") return;
      const accuracy = typeof p.accuracy === "number" ? p.accuracy : null;
      qc.setQueryData(queryKeys.game(p.gameId), (old: unknown) =>
        isRecord(old) ? { ...old, accuracy, analysisStatus: "done" } : old,
      );
      void qc.invalidateQueries({ queryKey: queryKeys.games(e.accountId), exact: true });
      return;
    }
    case "job.failed": {
      if (!isRecord(p) || typeof p.kind !== "string" || typeof p.reason !== "string") return;
      const { kind, reason } = p;
      updateActivity(qc, (a) => {
        const imports = { ...a.imports };
        if (isImportKind(kind)) delete imports[e.accountId];
        const failures = [...a.failures, { kind, reason, at: Date.now() }].slice(-MAX_FAILURES);
        return { imports, failures };
      });
      const label = kindLabel(kind);
      // A toast, never a modal: background work failing must not interrupt what the user does.
      toast(`${article(label)} ${label} job failed: ${reason}`);
      return;
    }
    case "reset":
      void qc.invalidateQueries();
      return;
    default:
      return;
  }
}
