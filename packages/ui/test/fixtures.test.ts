import { Chess } from "chess.js";
import { describe, expect, it } from "vitest";
import { areaScoreFixtures, games, kpiFixtures, timelineFixtures } from "../src/fixtures/index";
import { gameFixtureSchema } from "../src/fixtures/schema";

const all = Object.entries(games);

describe("game fixtures", () => {
  it.each(all)("%s parses with the zod schema", (_name, game) => {
    expect(gameFixtureSchema.safeParse(game).success).toBe(true);
  });

  it.each(all)("%s numbers plies from 1 without gaps", (_name, game) => {
    game.plies.forEach((p, i) => {
      expect(p.ply).toBe(i + 1);
    });
  });

  it.each(all)("%s replays legally and reproduces every fenAfter", (_name, game) => {
    const chess = new Chess(game.startFen);
    for (const p of game.plies) {
      const move = chess.move({
        from: p.uci.slice(0, 2),
        to: p.uci.slice(2, 4),
        promotion: p.uci.slice(4) || undefined,
      });
      expect(move.san).toBe(p.san);
      expect(chess.fen()).toBe(p.fenAfter);
    }
  });

  it.each(all)("%s keeps evals in White's point of view", (_name, game) => {
    for (const p of game.plies) {
      expect(p.winWhite).toBeGreaterThanOrEqual(0);
      expect(p.winWhite).toBeLessThanOrEqual(100);
      if (p.cpWhite !== null)
        expect(Math.sign(p.cpWhite) * (p.winWhite - 50)).toBeGreaterThanOrEqual(-15);
    }
  });

  it("middlegameBlunder has at least one blunder", () => {
    expect(games.middlegameBlunder.plies.some((p) => p.judgement === "blunder")).toBe(true);
  });

  it("endgameGrind runs past 100 plies and reaches the endgame", () => {
    expect(games.endgameGrind.plies.length).toBeGreaterThan(100);
    expect(games.endgameGrind.plies.some((p) => p.phase === "endgame")).toBe(true);
  });

  it("shortAttack is a short game", () => {
    expect(games.shortAttack.plies.length).toBeLessThan(50);
  });

  it("only uses public lichess games", () => {
    for (const [, game] of all) {
      expect(game.source).toMatch(/^https:\/\/lichess\.org\/[A-Za-z0-9]{8}$/);
      expect([game.white.name, game.black.name]).toContain("DrNykterstein");
    }
  });
});

describe("synthetic fixtures", () => {
  it("areaScoreFixtures lists the seven areas in order", () => {
    expect(areaScoreFixtures.items.map((i) => i.area)).toEqual([
      "openings",
      "middlegame",
      "endgame",
      "tactics",
      "calculation",
      "time",
      "mindset",
    ]);
    expect(areaScoreFixtures.synthetic).toBe(true);
  });

  it("timelineFixtures uses all six states", () => {
    const states = new Set(timelineFixtures.series.flatMap((s) => s.segments.map((g) => g.state)));
    expect([...states].sort()).toEqual(
      ["active", "emerging", "improving", "not_an_issue", "relapsed", "resolved"].sort(),
    );
    expect(timelineFixtures.synthetic).toBe(true);
  });

  it("kpiFixtures are marked synthetic with ordered intervals", () => {
    expect(kpiFixtures.synthetic).toBe(true);
    for (const k of kpiFixtures.items) expect(k.interval.low).toBeLessThanOrEqual(k.interval.high);
  });
});
