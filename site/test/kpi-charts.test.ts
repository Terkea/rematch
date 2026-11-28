import { mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  arenaVersions,
  buildScorecard,
  changeReading,
  charts,
  decisionsFromChangelog,
  fmt,
  fmtInterval,
  loadBenchHistory,
  scorecardData,
  scorecardPage,
  shortVersion,
  sparkline,
  statusFor,
  validateHistory,
  verdictText,
  wilson,
  writeScorecard,
} from "../scripts/kpi-charts.mjs";
import { cleanContext, figureAlt, figureLabels, loadExperiments } from "../scripts/lab-lib.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");
const board = JSON.parse(read("lab/leaderboard.json"));
const champion = JSON.parse(read("algorithm/manifest.json")).id as string;
const changelog = read("algorithm/CHANGELOG.md");
const baseline = JSON.parse(read("bench/baseline.json"));
const FIXTURE = join(HERE, "fixtures", "bench-history.json");

type Row = {
  id: string;
  tier: string;
  status: string;
  value: number | null;
  interval: [number, number] | null;
  basis?: string;
};
const live = () => buildScorecard(board, null, { champion, changelog, baseline });
const withFixture = () =>
  buildScorecard(board, loadBenchHistory(FIXTURE), { champion, changelog, baseline });
const row = (m: { rows: Row[] }, id: string) => m.rows.find((r) => r.id === id) as Row;

// A colour written into the markup instead of a token: a hex colour, rgb() or a fill/stroke value.
// An issue link's text ("#107") is not a colour.
const RAW_COLOUR = /(?<![>\w/])#(?:[0-9a-f]{3}){1,2}\b|rgba?\(|(?:fill|stroke|color)="(?!url)/i;

describe("numbers", () => {
  it("gives Wilson's 80% interval", () => {
    const [lo, hi] = wilson(19, 30) as [number, number];
    expect(lo).toBeCloseTo(0.5164, 3);
    expect(hi).toBeCloseTo(0.7364, 3);
    expect(wilson(0, 0)).toBeNull();
    // Six of six still starts below 0.80: six players cannot show a type is found.
    expect((wilson(6, 6) as number[])[0]).toBeLessThan(0.8);
  });
  it("rounds an interval outward and writes a real minus", () => {
    expect(fmtInterval([0.5164, 0.7364])).toBe("[0.51, 0.74]");
    expect(fmt(-0.363333)).toBe("−0.363");
    expect(fmt(Number.NaN)).toBe("—");
    expect(fmtInterval(null)).toBeNull();
  });
  it("says meets or misses only when the whole interval does", () => {
    expect(statusFor([0.81, 0.9], { kind: "min", value: 0.8 })).toBe("meets");
    expect(statusFor([0.5, 0.74], { kind: "min", value: 0.8 })).toBe("misses");
    expect(statusFor([0.78, 0.9], { kind: "min", value: 0.8 })).toBe("unclear");
    expect(statusFor([0.09, 0.13], { kind: "max", value: 0.1 })).toBe("unclear");
    expect(statusFor([0.8165, 0.8535], { kind: "band", band: [0.75, 0.85] })).toBe("unclear");
    expect(statusFor(null, { kind: "min", value: 0.8 })).toBe("unmeasured");
  });
});

describe("versions and decisions", () => {
  it("keeps the versions the arena re-ran, oldest first, without lab-only ones", () => {
    const v = arenaVersions(board).map(shortVersion);
    expect(v).toHaveLength(7);
    expect(v.at(-1)).toBe(shortVersion(champion));
    expect(v).not.toContain("5.0.1");
  });
  it("reads which experiment each version shipped from the changelog", () => {
    const d = decisionsFromChangelog(changelog);
    expect(d).toMatchObject({
      "2.0.0": "EXP-0006",
      "3.0.0": "EXP-0007",
      "4.0.0": "EXP-0008",
      "5.0.0": "EXP-0015",
      "6.0.0": "EXP-0030",
    });
    expect(d["4.0.1"]).toBeUndefined();
  });
});

describe("the bench history", () => {
  it("accepts the committed fixture and rejects a bad record", () => {
    expect(validateHistory(JSON.parse(readFileSync(FIXTURE, "utf8")))).toEqual([]);
    const bad = {
      schema: "bench-history-v1",
      runs: [
        {
          schema: "run-record-v1",
          date: "x",
          algorithm_id: "v",
          tier: "t",
          metrics: { a: { value: "1", interval: [2, 1] } },
        },
      ],
    };
    const errs = validateHistory(bad);
    expect(errs.join(" ")).toMatch(/date/);
    expect(errs.join(" ")).toMatch(/interval/);
    expect(errs.join(" ")).toMatch(/level/);
    const p = join(mkdtempSync(join(tmpdir(), "bh-")), "h.json");
    writeFileSync(p, JSON.stringify(bad));
    expect(() => loadBenchHistory(p)).toThrow(/runs\[0\]/);
    expect(loadBenchHistory(join(tmpdir(), "no-such-history.json"))).toBeNull();
  });
});

describe("the scorecard from the real leaderboard", () => {
  const m = live();
  const page = scorecardPage(m);
  it("has a row for every KPI of the tree", () => {
    for (const id of [
      "north-star",
      "ns-2",
      "transfer",
      "d1a",
      "d1b",
      "d2",
      "d3",
      "d4",
      "d5",
      "d6",
      "d7",
      "determinism",
      "motif-precision",
      "peer-coverage",
    ]) {
      expect(row(m, id), id).toBeTruthy();
    }
  });
  it("keeps the gaps on the page as unmeasured rows that link their experiment or issue", () => {
    const unmeasured = m.rows.filter((r) => r.tier === "unmeasured").map((r) => r.id);
    expect(unmeasured).toEqual(["north-star", "ns-2", "transfer", "d3", "d5", "d7"]);
    const rows = page.match(/<li class="sc-row sc-unmeasured"[\s\S]*?<\/li>/g) ?? [];
    expect(rows).toHaveLength(6);
    for (const r of rows) {
      expect(r).toContain("Unmeasured");
      expect(r).toMatch(/docs\/experiments\/exp-\d{4}\/|issues\/\d+/);
      expect(r).not.toMatch(/sc-num/);
    }
  });
  it("states n and the interval on every measured rate", () => {
    const d1b = row(m, "d1b");
    expect(d1b.value).toBeCloseTo(19 / 30, 6);
    expect(d1b.basis).toContain("19 of 30");
    expect(d1b.status).toBe("misses");
    expect(page).toContain('0.633</span><span class="sc-sub">[0.51, 0.74], 80%</span>');
    for (const r of m.rows.filter((r) => r.tier !== "unmeasured" && r.interval)) {
      expect(r.basis, r.id).toBeTruthy();
    }
  });
  it("labels an older metric as such", () => {
    expect(page).toContain("Shown as pooled recall at 2");
    expect(page).toContain("mean games to a detection is 42");
    expect(row(m, "d6").status).toBe("unjudged");
  });
  it("calls D4 a miss: detection fell at the version that cut the false alarms", () => {
    const d4 = row(m, "d4");
    expect(d4.status).toBe("misses");
    expect(page).toContain("0.887 to 0.523");
    expect(page).toContain("EXP-0015");
    expect(page).toContain("EXP-0030");
  });
  it("leads D1a with what the intervals show, not the point estimates", () => {
    // Six players a type: no type is clearly at 0.80, three are at it by point estimate alone.
    expect(row(m, "d1a").value).toBe(0);
    expect(row(m, "d1a").status).toBe("misses");
    expect(page).toContain("3 of 5 at or above 0.80 by point estimate alone");
  });
  it("never calls a change whose interval excludes 0 no clear difference", () => {
    // EXP-0015's 200-point guard passes on its floor, but the versions' interval separates.
    expect(page).toContain(
      "Detection of a 200-point fall: 6.0.0 is lower than 5.0.0 (the paired change −0.130, 80% interval [−0.15, −0.11], which excludes 0). The arena's guard passed only because it checks a floor of 0.5, which 6.0.0 clears (0.978 to 0.848).",
    );
    expect(page).not.toContain("6.0.0 is no clear difference from 5.0.0 (the paired change −0.130");
  });
  it("derives every verdict's words from its interval, for every comparison on the board", () => {
    const records = board.records.filter(
      (r: { algorithm_id: string; against: string | null }) =>
        r.algorithm_id === champion && r.against && r.against !== champion,
    );
    expect(records.length).toBeGreaterThan(0);
    for (const r of records) {
      for (const [name, c] of Object.entries(
        r.comparison as Record<string, { interval: [number, number] | null }>,
      )) {
        const text = verdictText(m, r.experiment, name, name) as string;
        const reading = changeReading(c);
        if (reading === "higher" || reading === "lower") {
          expect(text, `${r.experiment} ${name}`).not.toContain("no clear difference");
          expect(text, `${r.experiment} ${name}`).toContain(`is ${reading} than`);
        } else if (reading === "no clear difference") {
          expect(text, `${r.experiment} ${name}`).toContain("no clear difference");
        }
      }
    }
  });
  it("reads a rule's 'no clear difference' by the interval when the two disagree", () => {
    // A hand-written verdict that contradicts its own interval: the interval wins.
    const fake = structuredClone(board);
    const rec = fake.records.find(
      (r: { experiment: string; algorithm_id: string }) =>
        r.experiment === "EXP-0001" && r.algorithm_id === champion,
    );
    rec.comparison.recall_at_2 = {
      effect: 0.1,
      interval: [0.02, 0.18],
      level: 0.8,
      verdict: "no clear difference",
    };
    const fm = buildScorecard(fake, null, { champion, changelog, baseline });
    expect(verdictText(fm, "EXP-0001", "recall_at_2", "Recall")).toMatch(
      /is higher than .* which excludes 0/,
    );
    rec.comparison.recall_at_2 = {
      effect: -0.2,
      interval: [-0.3, 0.1],
      level: 0.8,
      verdict: "worse",
    };
    expect(verdictText(fm, "EXP-0001", "recall_at_2", "Recall")).toContain("no clear difference");
  });
  it("never writes 'no clear difference' beside an interval that excludes 0, anywhere on the page", () => {
    for (const p of [page, scorecardPage(withFixture())]) {
      const sentences = p.replace(/<[^>]+>/g, " ").split(/(?<=[.;])\s+/);
      for (const s of sentences.filter((x) => /no clear difference/.test(x))) {
        for (const iv of s.matchAll(/\[(−?\d+\.\d+), (−?\d+\.\d+)\]/g)) {
          const lo = Number((iv[1] ?? "").replace("−", "-"));
          const hi = Number((iv[2] ?? "").replace("−", "-"));
          expect(lo <= 0 && hi >= 0, s).toBe(true);
        }
      }
    }
  });
  it("draws both halves of the trajectory trade-off in the D4 row", () => {
    const d4 = page.match(/<li class="sc-row sc-synthetic" id="kpi-d4"[\s\S]*?<\/li>/)?.[0] ?? "";
    expect(d4).toContain("false alarms");
    expect(d4).toContain("100-point falls caught");
  });
  it("writes no overclaim, no NaN and no raw colour", () => {
    expect(page).not.toMatch(/NaN|undefined|Infinity|\bimproved\b/);
    expect(page).not.toMatch(RAW_COLOUR);
    expect(page).toContain("no clear difference");
  });
  it("draws every chart wide and narrow, with a title and a categorical version axis", () => {
    const c = charts(m) as Record<string, { wide: string; narrow: string }>;
    expect(Object.keys(c)).toEqual([
      "chart-recall",
      "chart-flaws",
      "chart-calibration",
      "chart-trajectory",
      "chart-motifs",
      "chart-depth",
    ]);
    for (const [id, svg] of Object.entries(c)) {
      for (const s of [svg.wide, svg.narrow]) {
        expect(s, id).toMatch(/^<svg [^>]*role="img"/);
        expect(s, id).toContain("<title");
        expect(s, id).not.toMatch(RAW_COLOUR);
      }
      expect(svg.wide, id).toContain(">6.0.0<");
    }
    // The trade-off scatter: an arrow from the old rule to the new one, both intervals as crosses.
    expect(c["chart-trajectory"]?.wide).toContain("marker-end");
    expect(c["chart-trajectory"]?.wide).toMatch(
      /told “falling” 0\.110 \[0\.09, 0\.13\] of 700; caught 0\.523 \[0\.49, 0\.55\] of 600/,
    );
    // A version an experiment did not run on shows a dash, never a zero.
    expect(c["chart-depth"]?.wide).toContain("not run on this version");
    // The rules mark the shipped decisions.
    expect(c["chart-recall"]?.wide).toContain("6.0.0 shipped EXP-0030");
  });
  it("draws a sparkline that reads its range, and says when there is too little", () => {
    expect(sparkline("x", [{ value: 0.1 }, null, { value: 0.2 }])).toContain(
      "2 of 3 versions measured",
    );
    expect(sparkline("x", [{ value: 0.1 }])).toContain("Fewer than two");
  });
  it("writes the SVGs and the data JSON", () => {
    const dir = mkdtempSync(join(tmpdir(), "sc-"));
    writeScorecard(dir, m);
    expect(readdirSync(join(dir, "scorecard"))).toContain("chart-trajectory.svg");
    const data = JSON.parse(readFileSync(join(dir, "scorecard.json"), "utf8"));
    expect(data.champion).toBe(champion);
    expect(data.rows.find((r: Row) => r.id === "d2").interval).toHaveLength(2);
    expect(scorecardData(m).fixture).toBe(false);
  });
});

describe("the scorecard from the bench fixture", () => {
  const m = withFixture();
  const page = scorecardPage(m);
  it("fills the bench rows, labelled as a fixture and never as a measurement", () => {
    expect(row(m, "d3").tier).toBe("fixture");
    expect(row(m, "d3").status).toBe("unclear");
    expect(row(m, "d5").value).toBe(41);
    expect(row(m, "d7").status).toBe("unclear");
    expect(page).toContain("come from a test fixture. They are not measurements.");
    expect(page).toContain("Macro recall at 2".replace("Macro", "macro"));
    expect(page).toContain("chart-cost");
  });
  it("still keeps the real-player rows unmeasured", () => {
    expect(m.rows.filter((r) => r.tier === "unmeasured").map((r) => r.id)).toEqual([
      "north-star",
      "ns-2",
      "transfer",
    ]);
  });
  it("writes no NaN and no raw colour", () => {
    expect(page).not.toMatch(/NaN|undefined|Infinity/);
    expect(page).not.toMatch(RAW_COLOUR);
  });
});

describe("experiment figures", () => {
  it("names what a figure plots from the analysis's labels, a loop's values included", () => {
    const py = [
      "def fig(x):",
      '    for ax, side in zip(axes, ("falling", "rising"), strict=True):',
      '        ax.set_xlabel("rated games")',
      "        ax.set_title(f'constant strength told \"{side}\"')",
      '    axes[0].set_ylabel("share of players")',
      '    fig.savefig(fig_dir / "false-alarms.png", dpi=120)',
    ].join("\n");
    const labels = figureLabels(py, "false-alarms.png");
    expect(figureAlt("EXP-0030", "false-alarms.png", labels, "")).toBe(
      'EXP-0030 figure: Share of players against rated games, panels: constant strength told "falling"; constant strength told "rising".',
    );
  });
  it("falls back to the sentence that names the figure, without its path", () => {
    expect(
      cleanContext(
        "Figure: figures/contrasts.png (the exact contrasts, from results.json);",
        "contrasts.png",
      ),
    ).toBe("The exact contrasts, from results.json");
    expect(
      cleanContext(
        "Results: results.json (commit d9474b6e), the figure in figures/x.png;",
        "x.png",
      ),
    ).toBe("");
  });
  it("gives every real figure alt text", () => {
    const figs = loadExperiments(join(ROOT, "lab")).flatMap((e) => e.figures);
    expect(figs.length).toBeGreaterThanOrEqual(10);
    for (const f of figs) expect(f.alt.length, f.file).toBeGreaterThan(30);
  });
});
