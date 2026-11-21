import { Chess } from "chess.js";
import { describe, expect, it } from "vitest";
import { type CoachingInput, parseCoachingInput } from "../../src/index.js";
import { loadFixture, makeInput } from "../fixtures/make-input.js";

const FIXTURES = ["b2-rapid", "b0-blitz", "early-read"];
const w1 = () => loadFixture("b2-rapid").weaknesses[0] as CoachingInput["weaknesses"][number];

function withExemplar(patch: Record<string, unknown>) {
  const f = loadFixture("b2-rapid");
  const w = f.weaknesses[0] as CoachingInput["weaknesses"][number];
  const ex = w.exemplars[0] as CoachingInput["weaknesses"][number]["exemplars"][number];
  w.exemplars[0] = { ...ex, ...patch } as typeof ex;
  return f;
}

describe("parseCoachingInput", () => {
  it.each(FIXTURES)("accepts fixture %s", (name) => {
    const r = parseCoachingInput(loadFixture(name));
    expect(r).toMatchObject({ ok: true });
  });

  it("validates the weakness id pattern", () => {
    expect(parseCoachingInput(makeInput({ weaknesses: [{ ...w1(), id: "W13" }] })).ok).toBe(false);
    expect(parseCoachingInput(makeInput({ weaknesses: [{ ...w1(), id: "W9b" }] })).ok).toBe(true);
    expect(parseCoachingInput(makeInput({ weaknesses: [{ ...w1(), id: "W0" }] })).ok).toBe(false);
    expect(parseCoachingInput(makeInput({ weaknesses: [{ ...w1(), id: "W12d" }] })).ok).toBe(true);
  });

  it("requires every area exactly once", () => {
    const b2 = loadFixture("b2-rapid");
    const missing = { ...b2, areaScores: b2.areaScores.slice(1) };
    expect(parseCoachingInput(missing)).toMatchObject({
      ok: false,
      issues: [{ path: "areaScores" }],
    });
    const dup = { ...b2, areaScores: [...b2.areaScores.slice(1), b2.areaScores[1]] };
    expect(parseCoachingInput(dup).ok).toBe(false);
  });

  it("rejects unknown keys and names them", () => {
    const r = parseCoachingInput({ ...loadFixture("b2-rapid"), focus: [] });
    expect(r).toMatchObject({ ok: false });
    if (!r.ok) expect(JSON.stringify(r.issues)).toContain("focus");
  });

  it("rejects an invalid FEN, a bad UCI and an out-of-range confidence", () => {
    expect(parseCoachingInput(withExemplar({ fen: "not a fen" })).ok).toBe(false);
    const ex = w1().exemplars[0] as CoachingInput["weaknesses"][number]["exemplars"][number];
    expect(parseCoachingInput(withExemplar({ played: { ...ex.played, uci: "e2e9" } })).ok).toBe(
      false,
    );
    expect(parseCoachingInput(makeInput({ weaknesses: [{ ...w1(), confidence: 1.2 }] })).ok).toBe(
      false,
    );
  });

  it("carries exemplars on strengths", () => {
    const b2 = loadFixture("b2-rapid");
    expect(b2.strengths[0]?.exemplars.length).toBeGreaterThan(0);
    const bad = makeInput({ strengths: [{ exemplars: [] }] });
    expect(parseCoachingInput(bad).ok).toBe(true);
  });

  it("limits exemplars to 5 per weakness", () => {
    const f = loadFixture("b2-rapid");
    const w = f.weaknesses[0] as CoachingInput["weaknesses"][number];
    const e = w.exemplars[0] as (typeof w.exemplars)[number];
    w.exemplars = [e, e, e, e, e, e];
    expect(parseCoachingInput(f).ok).toBe(false);
  });
});

describe("fixtures", () => {
  it("match the brief's shape", () => {
    const b2 = loadFixture("b2-rapid");
    expect(b2.weaknesses).toHaveLength(6);
    expect(b2.weaknesses.find((w) => w.id === "W1")?.exemplars).toHaveLength(3);
    const w3 = b2.weaknesses.find((w) => w.id === "W3");
    expect(w3?.variant).toBe("fork");
    expect(w3?.exemplars).toHaveLength(3);
    expect(b2.openings).toHaveLength(3);
    expect(b2.openings.some((o) => o.verdict === "fix" && o.deviationPly !== null)).toBe(true);
    expect(b2.changes).toHaveLength(2);
    expect(b2.basis).toMatchObject({ analysedGames: 214, importedGames: 3912 });
    expect(b2.timeControl).toEqual({ baseSeconds: 600, incrementSeconds: 5 });
    expect(b2.rating).toBe(1450);
    expect(loadFixture("b0-blitz").band).toBe("B0");
    const er = loadFixture("early-read");
    expect(er.basis.analysedGames).toBe(30);
    expect(er.areaScores.filter((a) => !a.sufficient && a.needMore !== null)).toHaveLength(3);
    expect(er.weaknesses.every((w) => w.confidence < 0.7)).toBe(true);
    expect(er.openings).toEqual([]);
  });

  it("makeInput deep-merges onto b2-rapid", () => {
    const m = makeInput({ band: "B3", basis: { analysedGames: 99 } });
    expect(m.band).toBe("B3");
    expect(m.basis.analysedGames).toBe(99);
    expect(m.basis.importedGames).toBe(3912);
    expect(m.weaknesses).toHaveLength(6);
  });

  it.each(FIXTURES)("every PV in %s replays legally", (name) => {
    const f = loadFixture(name);
    const exemplars = [
      ...f.weaknesses.flatMap((w) => w.exemplars),
      ...f.strengths.flatMap((s) => s.exemplars),
      ...f.openings.flatMap((o) => o.examples),
      ...f.changes.flatMap((c) => c.examples),
    ];
    expect(exemplars.length).toBeGreaterThan(0);
    const play = (c: Chess, uci: string) =>
      c.move({ from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] });
    for (const e of exemplars) {
      const best = new Chess(e.fen);
      for (const u of e.bestPv) expect(() => play(best, u), `${e.gameId} bestPv`).not.toThrow();
      const ref = new Chess(e.fen);
      play(ref, e.played.uci);
      expect(ref.history()[0]).toBe(e.played.san);
      for (const u of e.refutationPv)
        expect(() => play(ref, u), `${e.gameId} refutation`).not.toThrow();
      if (e.best) expect(new Chess(e.fen).move(e.best.san).san).toBe(e.best.san);
    }
  });
});

// biome-ignore lint/suspicious/noExplicitAny: mutation helper over untyped raw input
type Raw = Record<string, any>;
const raw = (): Raw => structuredClone(loadFixture("b2-rapid")) as unknown as Raw;
const issuePaths = (r: ReturnType<typeof parseCoachingInput>) =>
  r.ok ? [] : r.issues.map((i) => i.path);

describe("prompt-size bounds", () => {
  const cases: [string, (d: Raw) => void, string][] = [
    [
      "oversize PV",
      (d) => (d.weaknesses[0].exemplars[0].bestPv = Array(17).fill("e2e4")),
      "weaknesses.0.exemplars.0.bestPv",
    ],
    [
      "too many tags",
      (d) => (d.weaknesses[0].exemplars[0].tags = Array(13).fill("t")),
      "weaknesses.0.exemplars.0.tags",
    ],
    [
      "oversize tag",
      (d) => (d.weaknesses[0].exemplars[0].tags = ["x".repeat(41)]),
      "weaknesses.0.exemplars.0.tags.0",
    ],
    ["too many metrics", (d) => (d.metrics = Array(81).fill(d.metrics[0])), "metrics"],
    ["too many weaknesses", (d) => (d.weaknesses = Array(25).fill(d.weaknesses[0])), "weaknesses"],
    ["too many openings", (d) => (d.openings = Array(31).fill(d.openings[0])), "openings"],
    ["too many changes", (d) => (d.changes = Array(21).fill(d.changes[0])), "changes"],
    [
      "too many alternatives",
      (d) => (d.openings[1].alternatives = Array(6).fill(d.openings[1].alternatives[0])),
      "openings.1.alternatives",
    ],
    [
      "too many causes",
      (d) => (d.changes[0].candidateCauses = Array(7).fill(d.changes[0].candidateCauses[0])),
      "changes.0.candidateCauses",
    ],
    [
      "too many moves",
      (d) => (d.openings[0].movesSan = Array(41).fill("e4")),
      "openings.0.movesSan",
    ],
    ["oversize label", (d) => (d.weaknesses[0].label = "x".repeat(121)), "weaknesses.0.label"],
    [
      "oversize description",
      (d) => (d.changes[0].candidateCauses[0].description = "x".repeat(601)),
      "changes.0.candidateCauses.0.description",
    ],
    ["oversize rank", (d) => (d.weaknesses[0].rank = 25), "weaknesses.0.rank"],
    ["oversize priority", (d) => (d.weaknesses[0].priority = 1e9), "weaknesses.0.priority"],
  ];
  it.each(cases)("rejects %s with a named issue", (_n, mutate, path) => {
    const d = raw();
    mutate(d);
    expect(issuePaths(parseCoachingInput(d))).toContain(path);
  });
});

describe("documented ranges and cross-field rules", () => {
  it("rejects a per-game value declared as a rate", () => {
    const d = raw();
    d.metrics[0].unit = "rate";
    d.metrics[0].value = 3.1;
    expect(issuePaths(parseCoachingInput(d))).toContain("metrics.0.value");
  });
  it("rejects old unit names", () => {
    const d = raw();
    d.metrics[0].unit = "percent";
    expect(parseCoachingInput(d).ok).toBe(false);
  });
  it("rejects pct above 100 and interval fields out of 0..1", () => {
    const d = raw();
    d.metrics[3].value = 101;
    d.weaknesses[0].intervalHigh = 1.5;
    const paths = issuePaths(parseCoachingInput(d));
    expect(paths).toContain("metrics.3.value");
    expect(paths).toContain("weaknesses.0.intervalHigh");
  });
  it("requires metric low <= high", () => {
    const d = raw();
    d.metrics[0].low = 2;
    d.metrics[0].high = 1;
    expect(issuePaths(parseCoachingInput(d))).toContain("metrics.0.low");
  });
  it("requires intervalLow <= posteriorRate <= intervalHigh", () => {
    const d = raw();
    d.weaknesses[0].posteriorRate = 0.5;
    expect(issuePaths(parseCoachingInput(d))).toContain("weaknesses.0.posteriorRate");
    const e = raw();
    e.weaknesses[0].intervalLow = 0.3;
    expect(issuePaths(parseCoachingInput(e))).toContain("weaknesses.0.intervalLow");
  });
  it("requires unique weakness ranks", () => {
    const d = raw();
    d.weaknesses[1].rank = d.weaknesses[0].rank;
    expect(issuePaths(parseCoachingInput(d))).toContain("weaknesses.1.rank");
  });
  it("requires FEN side to move to match ply parity", () => {
    const d = raw();
    d.weaknesses[0].exemplars[0].ply += 1;
    expect(issuePaths(parseCoachingInput(d))).toContain("weaknesses.0.exemplars.0.fen");
    const c = raw();
    c.changes[0].examples[0].ply += 1;
    expect(issuePaths(parseCoachingInput(c))).toContain("changes.0.examples.0.fen");
  });
  it("requires events <= opportunities, onset ordering, analysed <= imported", () => {
    const d = raw();
    d.weaknesses[0].events = d.weaknesses[0].opportunities + 1;
    d.changes[0].onsetFrom = "2026-12-01";
    d.basis.analysedGames = d.basis.importedGames + 1;
    const paths = issuePaths(parseCoachingInput(d));
    expect(paths).toContain("weaknesses.0.events");
    expect(paths).toContain("changes.0.onsetDate");
    expect(paths).toContain("basis.analysedGames");
  });
});

describe("strictness at depth and change examples", () => {
  it("rejects nested unknown keys and names them", () => {
    for (const mutate of [
      (d: Raw) => (d.basis.extra = 1),
      (d: Raw) => (d.weaknesses[0].exemplars[0].played.extra = 1),
      (d: Raw) => (d.weaknesses[0].exemplars[0].evalBefore.extra = 1),
      (d: Raw) => (d.changes[0].candidateCauses[0].extra = 1),
      (d: Raw) => (d.openings[1].alternatives[0].extra = 1),
      (d: Raw) => (d.changes[0].examples[0].extra = 1),
    ]) {
      const d = raw();
      mutate(d);
      const r = parseCoachingInput(d);
      expect(r.ok).toBe(false);
      if (!r.ok) expect(r.issues.some((i) => i.message.includes("extra"))).toBe(true);
    }
  });
  it("requires a valid side on change examples and keeps 2 before / 2 after", () => {
    const b2 = loadFixture("b2-rapid");
    expect(b2.changes[0]?.examples.map((e) => e.side)).toEqual([
      "before",
      "before",
      "after",
      "after",
    ]);
    const d = raw();
    d.changes[0].examples[0].side = "during";
    expect(issuePaths(parseCoachingInput(d))).toContain("changes.0.examples.0.side");
    const m = raw();
    delete m.changes[0].examples[0].side;
    expect(parseCoachingInput(m).ok).toBe(false);
  });
});

describe("fixture consistency", () => {
  const all = () => {
    const f = loadFixture("b2-rapid");
    return f.weaknesses.flatMap((w) => w.exemplars.map((e) => ({ w: w.id, e })));
  };
  it("uses distinct positions across weaknesses", () => {
    const fens = all().map(({ e }) => e.fen);
    expect(new Set(fens).size).toBe(fens.length);
  });
  it("derives win percentages from evals for the mover, and the move costs the mover", () => {
    const win = (cp: number) => 50 + 50 * (2 / (1 + Math.exp(-0.00368208 * cp)) - 1);
    for (const { e } of all()) {
      const white = e.ply % 2 === 1;
      const cb = (e.evalBefore as { cp: number }).cp;
      const ca = (e.evalAfter as { cp: number }).cp;
      const sign = white ? 1 : -1;
      expect(e.winBefore).toBeCloseTo(win(sign * cb), 0);
      expect(e.winAfter).toBeCloseTo(win(sign * ca), 0);
      expect(e.winAfter).toBeLessThan(e.winBefore);
    }
  });
  it("b0 and b2 have different area scores and metrics", () => {
    const b2 = loadFixture("b2-rapid");
    const b0 = loadFixture("b0-blitz");
    expect(b0.areaScores.map((a) => a.score)).not.toEqual(b2.areaScores.map((a) => a.score));
    expect(b0.metrics[0]?.value).not.toBe(b2.metrics[0]?.value);
  });
});
