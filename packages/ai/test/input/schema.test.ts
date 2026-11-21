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
