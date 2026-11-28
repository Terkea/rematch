import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  experimentPage,
  experimentsIndex,
  labStrip,
  lagging,
  leaderboardSection,
  loadExperiments,
  loadLeaderboard,
  mathPage,
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
    expect(page).toContain("Live: re-run against the champion on every pull request");
    expect(experimentPage(exps[0] as never)).not.toContain("Live:");
  });
  it("links the maths page when the experiment has a math.md", () => {
    expect(exps[1]?.files).toMatchObject({ math: true });
    expect(experimentPage(exps[1] as never)).toContain(
      "[The maths](/rematch/docs/experiments/exp-9001/math/)",
    );
    expect(experimentPage(exps[0] as never)).not.toContain("The maths");
  });
  it('puts plain.md first as "In plain words", its title dropped, headings demoted, escaped', () => {
    expect(exps[1]?.files).toMatchObject({ plain: true });
    expect(exps[0]?.files).toMatchObject({ plain: false });
    const page = experimentPage(exps[1] as never);
    const plain = page.indexOf("## In plain words");
    expect(plain).toBeGreaterThan(-1);
    expect(plain).toBeLessThan(page.indexOf("- A second bullet"));
    expect(page).toContain("### What we asked");
    expect(page).not.toContain("# EXP-9001 in plain words");
    expect(page).toContain("with a &lt;tag> to escape");
    expect(page).toContain("blob/main/lab/experiments/EXP-9001/decision.md");
    expect(experimentPage(exps[0] as never)).not.toContain("In plain words");
  });
  it("builds the maths page from math.md, its links rewritten and its comments dropped", () => {
    const text = readFileSync(join(FIXTURE, "experiments", "EXP-9001", "math.md"), "utf8");
    const page = mathPage(exps[1] as never, text);
    expect(page).not.toMatch(/^# /m);
    expect(page).not.toContain("<!--");
    expect(page).toContain("## 4. Derivations and proofs");
    expect(page).toContain("blob/main/docs/experiments/MATH-TEMPLATE.md");
    expect(page).toContain("lab/experiments/EXP-9001/math.md");
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

describe("the leaderboard", () => {
  const exps = loadExperiments(FIXTURE) as never[];
  const board = loadLeaderboard(FIXTURE);
  it("reads the live flag and the board", () => {
    expect(loadExperiments(FIXTURE).find((e) => e.id === "EXP-9001")?.live).toBe(true);
    expect(loadExperiments(FIXTURE).find((e) => e.id === "EXP-9002")?.live).toBe(false);
    expect(board?.records).toHaveLength(2);
    expect(loadLeaderboard(mkdtempSync(join(tmpdir(), "lab-")))).toBeNull();
  });
  it("has a row per version, the champion first and marked, a column per live primary", () => {
    const html = leaderboardSection(exps, board as never, "algo-0.0.1");
    expect(html).toContain("## Leaderboard");
    const rows = html.match(/<tr[ >][\s\S]*?<\/tr>/g) ?? [];
    expect(rows).toHaveLength(3); // the header and two versions
    expect(rows[1]).toContain('class="lab-champion"');
    expect(rows[1]).toContain("algo-0.0.1");
    expect(rows[1]).toContain("Champion");
    expect(rows[2]).toContain("algo-0.1.0");
    expect(rows[2]).toContain("0.767 [0.600, 0.900]");
    expect(rows[2]).toContain("better than the champion");
    expect(rows[0]).toContain("Recall at 2");
    expect(rows[0]).toContain("/rematch/docs/experiments/exp-9001/");
    expect(html).not.toContain("Guarded"); // guards are in each experiment's record, not here
    expect(html).not.toMatch(/NaN|undefined|Infinity/);
  });
  it("shows a dash, never a zero, for a version an experiment has not measured", () => {
    const partial = { ...board, records: board?.records.slice(1) } as never;
    const html = leaderboardSection(exps, partial, "algo-0.0.1");
    expect(html).toContain("Not run");
    expect(html).not.toMatch(/>0\.000/);
  });
  it("is empty without a board or a live experiment", () => {
    expect(leaderboardSection(exps, null, "algo-0.0.1")).toBe("");
    expect(leaderboardSection([], board as never, "algo-0.0.1")).toBe("");
  });
  it("is on the Experiments index", () => {
    expect(experimentsIndex(exps, board as never, "algo-0.0.1")).toContain("lab-leaderboard");
  });
});

describe("the real lab", () => {
  // The guard: the site cannot lag the lab.
  const lab = join(ROOT, "lab");
  it("has a decided summary for every decision.md", () => {
    expect(missingSummaries(lab)).toEqual([]);
    expect(lagging(loadExperiments(lab))).toEqual([]);
  });
  it("has the champion on the leaderboard for every live experiment", () => {
    const champion = JSON.parse(readFileSync(join(ROOT, "algorithm", "manifest.json"), "utf8")).id;
    const board = loadLeaderboard(lab);
    const live = loadExperiments(lab).filter((e) => e.live);
    expect(live.length).toBeGreaterThanOrEqual(2);
    for (const e of live) {
      expect(
        board?.records.some((r) => r.experiment === e.id && r.algorithm_id === champion),
        e.id,
      ).toBe(true);
    }
    expect(leaderboardSection(live as never, board as never, champion)).toContain(champion);
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
