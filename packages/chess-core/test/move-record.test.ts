import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { buildMoveRecords, type PositionEval } from "../src/move-record.js";
import { parsePgn } from "../src/pgn.js";

const read = (name: string) => readFileSync(new URL(`./fixtures/${name}`, import.meta.url), "utf8");

const game = parsePgn(read("scholars-mate.pgn"));

const evals: PositionEval[] = [
  ...[20, 30, 25, 30, 40, 50].map((cp) => ({ score: { cp }, bestUci: null, pv: [] })),
  { score: { mate: 1 }, bestUci: "h5f7", pv: ["h5f7"] },
  { score: { cp: 1000 }, bestUci: null, pv: [] },
];

describe("buildMoveRecords", () => {
  const records = buildMoveRecords(game, evals, { baseSeconds: 180, incrementSeconds: 0 });

  it("builds one record per ply", () => {
    expect(records.length).toBe(7);
    expect(records[0]).toMatchObject({ ply: 1, color: "white", san: "e4", uci: "e2e4" });
    expect(records[0]?.phase).toBe("opening");
  });

  it("judges the blunder that allows mate from the mover's point of view", () => {
    const nf6 = records[5];
    expect(nf6?.san).toBe("Nf6");
    expect(nf6?.color).toBe("black");
    expect(nf6?.judgement).toBe("blunder");
    expect(nf6?.deltaW).toBeGreaterThan(40);
    expect(nf6?.winBefore).toBeCloseTo(45.4, 1);
    expect(nf6?.decided).toBe(false);
  });

  it("does not penalise the mating move and marks it decided", () => {
    const mate = records[6];
    expect(mate?.judgement).toBeNull();
    expect(mate?.deltaW).toBe(0);
    expect(mate?.accuracy).toBe(100);
    expect(mate?.decided).toBe(true);
    expect(mate?.bestUci).toBe("h5f7");
  });

  it("computes time spent from the previous clock of the same colour", () => {
    expect(records[0]?.clockBefore).toBe(180);
    expect(records[0]?.timeSpent).toBe(2);
    expect(records[2]?.clockBefore).toBe(178);
    expect(records[2]?.timeSpent).toBe(3);
  });

  it("adds the increment back to time spent", () => {
    const inc = buildMoveRecords(game, evals, { baseSeconds: 180, incrementSeconds: 2 });
    expect(inc[0]?.timeSpent).toBe(4);
  });

  it("leaves clock fields null without clock info", () => {
    const none = buildMoveRecords(game, evals, null);
    expect(none.every((r) => r.clockBefore === null && r.timeSpent === null)).toBe(true);
  });

  it("leaves time spent null for games without clock comments", () => {
    const noClocks = parsePgn(read("no-clocks.pgn"));
    const out = buildMoveRecords(noClocks, evals, { baseSeconds: 180, incrementSeconds: 0 });
    expect(out.every((r) => r.timeSpent === null)).toBe(true);
  });

  it("rejects an evals array of the wrong length", () => {
    expect(() => buildMoveRecords(game, evals.slice(1), null)).toThrow();
  });
});
