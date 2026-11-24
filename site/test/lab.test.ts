import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  experimentPage,
  experimentsIndex,
  labStrip,
  lagging,
  loadExperiments,
  missingSummaries,
  parseProgress,
  parseSummary,
} from "../scripts/lab-lib.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "..");
const FIXTURE = join(HERE, "fixtures", "lab");

describe("experiments", () => {
  const exps = loadExperiments(FIXTURE);
  it("lists newest first and finds the linked files", () => {
    // `id` comes from the summary's front matter, which the JS generator spreads untyped.
    expect(exps.map((e) => (e as { id?: string }).id)).toEqual(["EXP-9002", "EXP-9001"]);
    expect(exps[1]?.files).toMatchObject({ decision: true, results: true, review: false });
  });
  it("accepts a quoted or hashed issue number and joins wrapped bullets", () => {
    expect(exps[0]?.issue).toBe("2");
    expect(exps[1]?.bullets[1]).toBe("A second bullet that continues on a second line.");
  });
  it("builds a page with the bullets, GitHub links and the issue", () => {
    const page = experimentPage(exps[1] as never);
    expect(page).toContain("lab-badge-decided");
    expect(page).toContain("blob/main/lab/experiments/EXP-9001/decision.md");
    expect(page).toContain("https://github.com/Terkea/rematch/issues/1");
    expect(page).not.toContain("/review.md");
  });
  it("builds an index with every status and a note when empty", () => {
    expect(experimentsIndex(exps as never)).toContain("/rematch/docs/experiments/exp-9002/");
    expect(experimentsIndex([])).toContain("No experiment has a summary yet");
  });
  it("tolerates a lab with no experiments folder", () => {
    expect(loadExperiments(mkdtempSync(join(tmpdir(), "lab-")))).toEqual([]);
  });
  it("rejects a bad status and missing fields", () => {
    const bad =
      "---\nid: EXP-1\ntitle: t\nstatus: done\nalgorithm: a\nissue: 1\nupdated: 2026-01-01\n---\n- x\n";
    expect(() => parseSummary(bad, "EXP-1")).toThrow(/status/);
    expect(() => parseSummary("no front matter")).toThrow(/front matter/);
  });
  it("flags a decision whose summary lags", () => {
    const dir = mkdtempSync(join(tmpdir(), "lab-"));
    const e = join(dir, "experiments", "EXP-0001");
    mkdirSync(e, { recursive: true });
    writeFileSync(join(e, "decision.md"), "ship");
    writeFileSync(
      join(e, "summary.md"),
      "---\nid: EXP-0001\ntitle: t\nstatus: running\nalgorithm: a\nissue: 1\nupdated: 2026-01-01\n---\n- x\n",
    );
    expect(lagging(loadExperiments(dir))).toEqual(["EXP-0001"]);
    writeFileSync(
      join(e, "summary.md"),
      "---\nid: EXP-0001\ntitle: t\nstatus: decided\nalgorithm: a\nissue: 1\nupdated: 2026-01-01\n---\n- x\n",
    );
    expect(lagging(loadExperiments(dir))).toEqual([]);
  });
});

describe("the real lab", () => {
  // The guard: the site cannot lag the lab.
  const lab = join(ROOT, "lab");
  it("has a decided summary for every decision.md", () => {
    expect(missingSummaries(lab)).toEqual([]);
    expect(lagging(loadExperiments(lab))).toEqual([]);
  });
});

describe("progress", () => {
  it("parses dated entries, newest first, ignoring comments", () => {
    const md =
      "# Progress\n<!-- rule -->\n## 2026-01-01: Old\n- a\n## 2026-02-01: New\n- b\n  more\n- c\n";
    const p = parseProgress(md);
    expect(p.map((m) => m.title)).toEqual(["New", "Old"]);
    expect(p[0]?.bullets).toEqual(["b more", "c"]);
  });
  it("seeds docs/progress.md with dated milestones and the contributor rule", async () => {
    const { readFileSync } = await import("node:fs");
    const text = readFileSync(join(ROOT, "docs", "progress.md"), "utf8");
    expect(text).toContain(
      "Add an entry when a release ships, an experiment is decided, or a milestone closes.",
    );
    const p = parseProgress(text);
    expect(p.length).toBeGreaterThanOrEqual(6);
    for (const m of p) expect(m.bullets.length).toBeGreaterThanOrEqual(2);
  });
});

describe("labStrip", () => {
  it("degrades to nothing", () => {
    expect(labStrip([], [])).toEqual({ experiments: [], milestone: null });
  });
  it("takes three experiments and the newest milestone, as plain text", () => {
    const exps = loadExperiments(FIXTURE) as never[];
    const s = labStrip(exps, parseProgress("## 2026-01-01: A [x](y)\n- one `two`\n"));
    expect(s.experiments).toHaveLength(2);
    expect(s.milestone).toMatchObject({ title: "A x", first: "one two" });
  });
});
