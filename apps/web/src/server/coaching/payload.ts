import type {
  AiDisclosure,
  ComputedReport,
  PayloadDroppedItem,
  ReportPayload,
  ReportRow,
  ReportSectionRow,
  SectionContent,
  SectionKey,
} from "./types";
import { SECTION_KEYS } from "./types";

/** `ReportPayload.schemaVersion` (Rust `REPORT_PAYLOAD_SCHEMA_VERSION`). */
export const REPORT_PAYLOAD_SCHEMA_VERSION = "report-payload-v1";

type StoredDropped = { section: string; path: string; errors: { code: string; path: string }[] };

/**
 * The TS port of Rust `load_report_payload` + `assemble_report_payload`
 * (`backend/crates/ai/src/generate/payload.rs`): sections in `SECTION_KEYS` order; a ready
 * section shows its content, a dropped one null, one not generated (yet) is absent; `dropped`
 * lists every section's items without their `detail` (no rejected model text); the status is
 * `complete` with all five ready, `computed_only` with none, else `partial`; the disclosure is
 * kept only when some section is ready; `generatedAt` is the report's `updated_at`.
 */
export function assembleReportPayload(
  report: ReportRow,
  sections: ReportSectionRow[],
): ReportPayload {
  if (report.schemaVersion !== REPORT_PAYLOAD_SCHEMA_VERSION) {
    throw new Error(
      `reports.schema_version is "${report.schemaVersion}", expected "${REPORT_PAYLOAD_SCHEMA_VERSION}"`,
    );
  }
  const byKey = new Map<SectionKey, ReportSectionRow>();
  for (const s of sections) {
    if ((SECTION_KEYS as readonly string[]).includes(s.section)) {
      byKey.set(s.section as SectionKey, s);
    }
  }
  const ordered = SECTION_KEYS.flatMap((k) => {
    const s = byKey.get(k);
    return s ? [[k, s] as const] : [];
  });

  const ready = ordered.filter(([, s]) => s.status === "ready" && s.content != null).length;
  const status: ReportPayload["status"] =
    ready === SECTION_KEYS.length ? "complete" : ready === 0 ? "computed_only" : "partial";

  const out: ReportPayload["sections"] = {};
  for (const [k, s] of ordered) {
    out[k] = s.status === "ready" ? ((s.content as SectionContent | null) ?? null) : null;
  }

  const dropped: PayloadDroppedItem[] = ordered.flatMap(([, s]) =>
    ((s.dropped ?? []) as StoredDropped[]).map((d) => ({
      section: d.section,
      path: d.path,
      errors: d.errors.map((e) => ({ code: e.code, path: e.path })),
    })),
  );

  return {
    schemaVersion: REPORT_PAYLOAD_SCHEMA_VERSION,
    reportId: report.id,
    version: report.version,
    status,
    computed: report.computed as ComputedReport,
    sections: out,
    dropped,
    aiDisclosure:
      status === "computed_only" ? null : ((report.aiDisclosure as AiDisclosure | null) ?? null),
    generatedAt: report.updatedAt.toISOString(),
  };
}
