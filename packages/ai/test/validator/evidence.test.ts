import { DEFAULT_POSITION as START_FEN } from "chess.js";
import { describe, expect, it } from "vitest";
import { buildEvidence, createEvidence } from "../../src/validator/evidence.js";
import { loadFixture, makeInput } from "../fixtures/make-input.js";

describe("buildEvidence on b2-rapid", () => {
  const input = loadFixture("b2-rapid");
  const ev = buildEvidence(input);
  const ev420 = buildEvidence(
    makeInput({ weaknesses: [{ exemplars: [{ evalAfter: { cp: -420 } }] }] }),
  );
  const w1ex0 = input.weaknesses[0]!.exemplars[0]!;

  it("indexes exemplar positions with played, best, and PV moves", () => {
    const p = ev.positions.get(`${w1ex0.gameId}#${w1ex0.ply}`)!;
    expect(p.allowedSan.has(w1ex0.played.san)).toBe(true);
    expect(p.allowedSan.has(w1ex0.best!.san)).toBe(true);
    expect(p.allowedSan.has("Nxd4")).toBe(true); // refutationPv[1]
    expect(p.allowedUci.has(w1ex0.played.uci)).toBe(true);
    expect(p.allowedUci.has("f3d4")).toBe(true);
    expect(ev.gameIds.has(w1ex0.gameId)).toBe(true);
  });

  it("indexes opening lines", () => {
    const o = input.openings[0]!;
    const line = ev.lines.get(o.lineKey)!;
    for (const m of o.movesSan) expect(line.has(m)).toBe(true);
    if (o.deviationMoveSan) expect(line.has(o.deviationMoveSan)).toBe(true);
    for (const a of o.alternatives) for (const m of a.movesSan) expect(line.has(m)).toBe(true);
  });

  it("collects numbers", () => {
    expect(ev.numbers).toContain(214);
    expect(ev.numbers).toContain(10); // 600 s base / 60
    expect(
      ev.numbers.some(
        (n) =>
          Math.abs(n - Math.abs("cp" in w1ex0.evalAfter ? w1ex0.evalAfter.cp : 0) / 100) < 1e-9,
      ),
    ).toBe(true);
    expect(ev420.numbers.some((n) => Math.abs(n - 4.2) < 1e-9)).toBe(true);
    expect(ev420.numbers.some((n) => Math.abs(n + 4.2) < 1e-9)).toBe(true);
    expect(ev.numbers).toContain(3); // ceil(6/2)
    expect(ev.numbers).toContain(input.weaknesses.length);
  });

  it("stores rates in [0,1] also times 100", () => {
    const r = input.weaknesses[0]!.posteriorRate;
    expect(ev.numbers.some((n) => Math.abs(n - r * 100) < 1e-6)).toBe(true);
  });

  it("collects ratios for changes both ways", () => {
    const c = input.changes[0]!;
    expect(ev.ratios.some((n) => Math.abs(n - c.rateAfter / c.rateBefore) < 1e-9)).toBe(true);
    expect(ev.ratios.some((n) => Math.abs(n - c.rateBefore / c.rateAfter) < 1e-9)).toBe(true);
  });

  it("collects dates as YYYY-MM-DD and MM-DD, plus their parts", () => {
    const c = input.changes[0]!;
    expect(ev.dates.has(c.onsetDate)).toBe(true);
    expect(ev.dates.has(c.onsetDate.slice(5))).toBe(true);
    expect(ev.numbers).toContain(Number(c.onsetDate.slice(0, 4)));
    expect(ev.numbers).toContain(Number(c.onsetDate.slice(8, 10)));
  });

  it("indexes change examples and onset games", () => {
    const c = input.changes[0]!;
    expect(ev.gameIds.has(c.onsetGameId)).toBe(true);
    const e = c.examples[0];
    if (e) expect(ev.positions.has(`${e.gameId}#${e.ply}`)).toBe(true);
  });
});

describe("addNumbersFrom", () => {
  it("never mines uuid strings, even under mined keys", () => {
    const ev = createEvidence();
    ev.addNumbersFrom({
      gameId: "4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9b",
      name: "4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9b",
      label: "4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9b",
    });
    expect(ev.numbers).toEqual([]);
  });
  it("mines strings only under label|description|name|before|after", () => {
    const ev = createEvidence();
    ev.addNumbersFrom({ label: "7 blunders", other: "99 problems", before: "5 games" });
    expect(ev.numbers).toContain(7);
    expect(ev.numbers).toContain(5);
    expect(ev.numbers).not.toContain(99);
  });
  it("stores score cp both signs and mate", () => {
    const ev = createEvidence();
    ev.addNumbersFrom({ evalBefore: { cp: 150 }, evalAfter: { mate: -3 } });
    expect(ev.numbers).toEqual(expect.arrayContaining([1.5, -1.5, 3, -3]));
  });
  it("adds ply as full-move number and array lengths", () => {
    const ev = createEvidence();
    ev.addNumbersFrom({ ply: 7, items: [1, 2, 3, 4, 5] });
    expect(ev.numbers).toContain(4);
    expect(ev.numbers).toContain(5);
  });
});

describe("addPosition", () => {
  it("never throws on bad PVs", () => {
    const ev = createEvidence();
    ev.addPosition({
      gameId: "g",
      ply: 1,
      fen: START_FEN,
      played: null,
      best: null,
      pvs: [{ fromFen: "bad", uci: ["e2e4"] }],
    });
    expect(ev.positions.get("g#1")!.allowedSan.size).toBe(0);
  });
});
