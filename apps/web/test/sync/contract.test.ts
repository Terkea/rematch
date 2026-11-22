import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ETA, FAIR_ORDER_SQL, POSITION_TAIL } from "../../src/server/sync/eta";
import { ANALYSE_JOB, IMPORT_JOB, PROFILE_KIND } from "../../src/server/sync/jobs";

// The web enqueues the same `import-account` and `analyse-game` jobs the Rust worker does, and
// mirrors the scheduler's ETA. Each value is pinned against the Rust source it copies.

const rust = (path: string) =>
  readFileSync(new URL(`../../../../backend/crates/${path}`, import.meta.url), "utf8");
const kinds = rust("queue/src/kinds.rs");
const importJob = rust("analysis/src/jobs/import.rs");
const analyseJob = rust("analysis/src/jobs/analyse.rs");
const types = rust("queue/src/types.rs");
const fair = rust("scheduler/src/fair.rs");
const eta = rust("scheduler/src/eta.rs");

const constant = (src: string, name: string) =>
  src.match(new RegExp(`pub const ${name}: [^=]+= ([^;]+);`))?.[1];

/** A Rust string literal's value: a `\` at a line end drops the newline and the next indent. */
const rustString = (literal: string) => literal.replace(/\\\r?\n\s*/g, "");

describe("import job contract with queue::kinds and analysis::jobs::import", () => {
  it("kind, dedupe key and payload match enqueue_incremental_import", () => {
    expect(constant(kinds, "KIND_IMPORT")).toBe(JSON.stringify(IMPORT_JOB.kind));
    expect(kinds).toContain('format!("import:{account_id}")');
    expect(IMPORT_JOB.key("a1")).toBe("import:a1");
    // `ImportAccountPayload` is camelCase; `max` is skipped when None, so no limit is sent.
    expect(kinds).toMatch(
      /#\[serde\(rename_all = "camelCase"\)\]\s*pub struct ImportAccountPayload \{\s*pub account_id: Uuid,\s*#\[serde\(skip_serializing_if = "Option::is_none"\)\]\s*pub max: Option<u32>,/,
    );
    expect(kinds).toMatch(/pub incremental: bool,/);
    const incremental = importJob.slice(
      importJob.indexOf("pub async fn enqueue_incremental_import"),
    );
    expect(incremental.slice(0, incremental.indexOf(".await"))).toMatch(
      /ImportAccountPayload \{\s*account_id,\s*max: None,\s*incremental: true,\s*\}/,
    );
    expect(IMPORT_JOB.payload("a1")).toEqual({ accountId: "a1", incremental: true });
  });

  it("attempts, priority and user key match enqueue_import", () => {
    expect(constant(importJob, "IMPORT_MAX_ATTEMPTS")).toBe(String(IMPORT_JOB.maxAttempts));
    const body = importJob.slice(importJob.indexOf("async fn enqueue_import_payload"));
    const options = body.slice(0, body.indexOf("..EnqueueOptions::default()"));
    expect(options).toContain("KIND_IMPORT");
    expect(options).toContain("dedupe_key: Some(import_key(account_id))");
    expect(options).toContain("user_key: Some(account_id)");
    expect(options).not.toContain("priority:");
    expect(types).toMatch(/priority: 100,/);
    expect(IMPORT_JOB.priority).toBe(100);
  });

  it("analyse job matches analyse_enqueue_options and the reconciler's priority", () => {
    expect(constant(kinds, "KIND_ANALYSE")).toBe(JSON.stringify(ANALYSE_JOB.kind));
    expect(kinds).toContain('format!("analyse:{game_id}")');
    expect(ANALYSE_JOB.key("g1")).toBe("analyse:g1");
    expect(kinds).toMatch(/pub struct AnalyseGamePayload \{\s*pub game_id: Uuid,\s*\}/);
    expect(ANALYSE_JOB.payload("g1")).toEqual({ gameId: "g1" });
    expect(constant(analyseJob, "ANALYSE_MAX_ATTEMPTS")).toBe(String(ANALYSE_JOB.maxAttempts));
    expect(constant(analyseJob, "DEFAULT_ANALYSE_PRIORITY")).toBe(
      String(ANALYSE_JOB.defaultPriority),
    );
    expect(rust("analysis/src/jobs/reconcile.rs")).toContain("coalesce(analysis_priority, 100)");
  });

  it("the profile kind matches KIND_BUILD_PROFILE", () => {
    expect(constant(kinds, "KIND_BUILD_PROFILE")).toBe(JSON.stringify(PROFILE_KIND));
  });
});

describe("ETA contract with scheduler::eta and scheduler::fair", () => {
  it("the fair order is FAIR_ORDER_SQL byte for byte", () => {
    const m = fair.match(/const FAIR_ORDER_SQL: &str = "((?:[^"\\]|\\.|\\\r?\n)*)";/);
    expect(m?.[1]).toBeDefined();
    expect(rustString(m?.[1] ?? "")).toBe(FAIR_ORDER_SQL);
  });

  it("the position query is position_sql's tail", () => {
    const m = eta.match(/format!\(\s*"\{fair\}((?:[^"\\]|\\.|\\\r?\n)*)",/);
    expect(m?.[1]).toBeDefined();
    expect(rustString(m?.[1] ?? "")).toBe(POSITION_TAIL);
  });

  it("thresholds, window and fair parameters match", () => {
    expect(constant(eta, "MIN_RATE_ROWS")).toBe(String(ETA.minRateRows));
    expect(Number(constant(eta, "RATE_WINDOW_SECS"))).toBe(ETA.rateWindowSecs);
    expect(constant(eta, "ETA_WINDOW")).toBe(String(ETA.window));
    expect(eta).toContain("created_at > now() - interval '10 minutes'");
    expect(ETA.rateWindowSecs).toBe(600);
    expect(constant(fair, "DEFAULT_SLICE")).toBe(String(ETA.slice));
    expect(constant(fair, "DEFAULT_SERVED_WINDOW")).toBe(
      `Duration::from_secs(${ETA.servedWindowMs / 1000})`,
    );
  });
});
