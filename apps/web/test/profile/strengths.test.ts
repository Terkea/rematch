import { describe, expect, it } from "vitest";
import {
  aboveBand,
  bandMedians,
  bandOf,
  CRITICAL_GAP,
  gameValues,
  HARD_P_BEST,
  type HardSpotCandidate,
  improvingWeaknesses,
  keepDoingNote,
  ownTrends,
  pickHardSpots,
  RECENT_GAMES,
  reportStrengths,
  workingOpenings,
} from "../../src/server/profile/strengths";
import { wilson } from "../../src/server/stats";

// The strengths read model's gates (pure): every claim on /app/strengths passes one of these,
// and each one refuses to claim without its minimum evidence.

describe("bandOf", () => {
  it("puts a chess.com rating on the Lichess scale before banding (Rust bands.rs)", () => {
    // chess.com rapid + 350: 1000 is 1350 on Lichess, band B2 (1300-1699).
    expect(bandOf("chesscom", "rapid", 1000)).toBe("B2");
    expect(bandOf("lichess", "rapid", 1000)).toBe("B1");
    expect(bandOf("chesscom", "blitz", 640)).toBe("B0");
    expect(bandOf("lichess", "blitz", 2300)).toBe("B5");
  });

  it("has no band without a rating", () => {
    expect(bandOf("lichess", "rapid", null)).toBeNull();
    expect(bandOf("lichess", "rapid", Number.NaN)).toBeNull();
  });
});

describe("bandMedians", () => {
  it("takes the nearest band within two, the lower one on ties (Rust load_band_reference)", () => {
    const m = bandMedians(
      [
        { band: "B0", metric: "accuracy", median: 60 },
        { band: "B4", metric: "accuracy", median: 80 },
        { band: "B1", metric: "blunders_per_game", median: 1.5 },
        { band: "B3", metric: "blunders_per_game", median: 0.9 },
        { band: "B5", metric: "conversion_rate", median: 0.9 },
      ],
      "B2",
    );
    expect(m.get("accuracy")).toEqual({ median: 60, band: "B0" });
    expect(m.get("blunders_per_game")).toEqual({ median: 1.5, band: "B1" });
    expect(m.get("conversion_rate")).toBeUndefined();
  });
});

describe("aboveBand", () => {
  const medians = new Map([
    ["accuracy", { median: 70, band: "B2" as const }],
    ["blunders_per_game", { median: 1.2, band: "B2" as const }],
    ["conversion_rate", { median: 0.6, band: "B2" as const }],
  ]);
  const row = (metric: string, value: number, low: number, high: number, n: number) => ({
    metric,
    value,
    low,
    high,
    n,
  });

  it("claims only when the whole interval is on the better side of the median", () => {
    const claims = aboveBand(
      [
        row("accuracy", 74, 71.2, 76.8, 40),
        row("blunders_per_game", 0.8, 0.6, 1.0, 40),
        row("conversion_rate", 0.7, 0.55, 0.82, 30),
      ],
      medians,
    );
    expect(claims.map((c) => c.metric)).toEqual(["accuracy", "blunders_per_game"]);
    expect(claims[1]).toMatchObject({ label: "Blunders per game", median: 1.2, n: 40 });
  });

  it("never claims from too few games, an interval that touches the median, or a worse side", () => {
    expect(
      aboveBand(
        [
          row("accuracy", 74, 71, 77, 12), // minN 20
          row("blunders_per_game", 1.0, 0.8, 1.2, 40), // touches 1.2
        ],
        medians,
      ),
    ).toEqual([]);
    expect(aboveBand([row("accuracy", 60, 58, 62, 40)], medians)).toEqual([]);
  });

  it("skips non-finite stored values", () => {
    expect(aboveBand([row("accuracy", Number.NaN, 71, 76, 40)], medians)).toEqual([]);
  });
});

describe("gameValues", () => {
  it("reads the per-game KPIs from game_metrics.metrics", () => {
    expect(
      gameValues({
        version: 1,
        accuracy: 81.5,
        accuracyByPhase: { opening: 90, middlegame: 75, endgame: null },
        blunders: 2,
        conversion: { episode: true, converted: false },
        resilience: { episode: false, saved: null },
      }),
    ).toEqual({
      accuracy: 81.5,
      "accuracy.opening": 90,
      "accuracy.middlegame": 75,
      blunders_per_game: 2,
      conversion_rate: 0,
    });
  });

  it("reads nothing from a malformed or empty row", () => {
    expect(gameValues(null)).toEqual({});
    expect(gameValues({ version: 1 })).toEqual({});
    expect(gameValues({ accuracy: "80", blunders: Number.POSITIVE_INFINITY })).toEqual({});
  });
});

describe("ownTrends", () => {
  /** Games oldest first: `earlier` games of one accuracy, then `RECENT_GAMES` of another. */
  const games = (earlier: number, before: number[], after: number[]) => [
    ...Array.from({ length: earlier }, (_, i) => ({
      accuracy: before[i % before.length] as number,
      blunders_per_game: 2,
    })),
    ...Array.from({ length: RECENT_GAMES }, (_, i) => ({
      accuracy: after[i % after.length] as number,
      blunders_per_game: 2,
    })),
  ];

  it("claims a KPI whose bootstrap difference interval is wholly on the better side", () => {
    const r = ownTrends(games(40, [60, 64, 66, 70], [72, 75, 78, 80]), "seed");
    expect(r.claims.map((c) => c.metric)).toEqual(["accuracy"]);
    const c = r.claims[0];
    expect(c?.recent.n).toBe(RECENT_GAMES);
    expect(c?.earlier.n).toBe(40);
    expect(c?.recent.value).toBeCloseTo(76.14);
    expect(c?.earlier.value).toBeCloseTo(65);
    expect(c?.diff.low).toBeGreaterThan(0);
    expect(r.enough).toBe(true);
  });

  it("does not claim a change whose interval spans zero, nor a constant metric", () => {
    const r = ownTrends(games(40, [60, 80], [62, 79]), "seed");
    expect(r.claims).toEqual([]);
    expect(r.enough).toBe(true);
  });

  it("says when there are too few earlier games to compare", () => {
    const r = ownTrends(games(12, [60], [80]), "seed");
    expect(r.claims).toEqual([]);
    expect(r.enough).toBe(false);
    expect(r.earlierGames).toBe(12);
    expect(r.needEarlier).toBe(8);
  });

  it("is deterministic for a seed", () => {
    const g = games(40, [60, 64, 66, 70], [72, 75, 78, 80]);
    expect(ownTrends(g, "a")).toEqual(ownTrends(g, "a"));
  });

  it("reads a lower-is-better KPI the right way round", () => {
    const g = [
      ...Array.from({ length: 30 }, (_, i) => ({ blunders_per_game: [2, 3, 2, 4][i % 4] as number })),
      ...Array.from({ length: RECENT_GAMES }, (_, i) => ({
        blunders_per_game: [0, 1, 0, 1][i % 4] as number,
      })),
    ];
    expect(ownTrends(g, "s").claims.map((c) => c.metric)).toEqual(["blunders_per_game"]);
  });
});

describe("improvingWeaknesses", () => {
  const base = {
    seriesKey: "W1",
    label: "Hung material",
    state: "improving" as const,
    sufficient: true,
    chances: 2400,
    transition: {
      toState: "improving" as const,
      pChange: 0.86,
      rateBefore: 0.04,
      rateAfter: 0.026,
      at: new Date("2026-10-14T12:00:00Z"),
      counts: { events: 21, chances: 800 },
    },
  };

  it("keeps a confirmed drop (p_change ≥ 0.8, a relative drop of at least 15%)", () => {
    expect(improvingWeaknesses([base])).toEqual([
      {
        seriesKey: "W1",
        label: "Hung material",
        state: "improving",
        before: 0.04,
        after: 0.026,
        pChange: 0.86,
        at: base.transition.at,
        afterCounts: { events: 21, chances: 800 },
        chances: 2400,
      },
    ]);
  });

  it("drops an unconfirmed, small, early or non-improving change", () => {
    const t = base.transition;
    expect(
      improvingWeaknesses([
        { ...base, transition: { ...t, pChange: 0.79 } },
        { ...base, transition: { ...t, pChange: null } },
        { ...base, transition: { ...t, rateAfter: 0.036 } }, // 10% drop
        { ...base, sufficient: false },
        { ...base, state: "active" },
        { ...base, transition: null },
        { ...base, transition: { ...t, rateBefore: 0, rateAfter: 0 } },
      ]),
    ).toEqual([]);
  });

  it("keeps a resolved one", () => {
    const r = improvingWeaknesses([
      { ...base, state: "resolved", transition: { ...base.transition, toState: "resolved" } },
    ]);
    expect(r.map((w) => w.state)).toEqual(["resolved"]);
  });
});

describe("workingOpenings", () => {
  const line = (n: number, wins: number, expected: number) => ({
    lineKey: "italian",
    name: "Italian Game",
    color: "white" as const,
    kind: "chosen" as const,
    verdict: "keep" as const,
    games: Array.from({ length: n }, (_, i) => ({ score: i < wins ? 1 : 0, expected })),
  });

  it("keeps a line whose Wilson interval sits wholly above its expected score", () => {
    const [o] = workingOpenings([line(20, 16, 0.5)]);
    const w = wilson(16, 20);
    expect(o).toMatchObject({ name: "Italian Game", n: 20, score: 0.8, expected: 0.5 });
    expect(o?.low).toBeCloseTo(w?.low ?? 0);
    expect(o?.verdict).toBe("keep");
  });

  it("refuses fewer than 10 games, or an interval that reaches the expected score", () => {
    expect(workingOpenings([line(9, 9, 0.5)])).toEqual([]);
    expect(workingOpenings([line(20, 12, 0.5)])).toEqual([]);
  });

  it("counts only games with both a score and an expected score", () => {
    const l = line(10, 10, 0.5);
    l.games.push({ score: Number.NaN, expected: 0.5 }, { score: 1, expected: Number.NaN });
    expect(workingOpenings([l])[0]?.n).toBe(10);
  });
});

describe("pickHardSpots", () => {
  const spot = (over: Partial<HardSpotCandidate>): HardSpotCandidate => ({
    gameId: "g1",
    playedAt: new Date("2026-10-01T12:00:00Z"),
    ply: 21,
    kind: "only_move_found",
    fen: "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    san: "Bb5",
    uci: "f1b5",
    bestUci: "f1b5",
    winBefore: 54,
    winAfter: 55,
    timeSpent: 12,
    difficultyStatus: "pending_maia",
    pBest: null,
    isCritical: true,
    multipvGap: 22,
    ...over,
  });

  it("prefers Maia-hard positions (difficulty ok, p_best at most HARD_P_BEST)", () => {
    const r = pickHardSpots([
      spot({ gameId: "a", difficultyStatus: "ok", pBest: HARD_P_BEST }),
      spot({ gameId: "b", difficultyStatus: "ok", pBest: 0.8 }),
      spot({ gameId: "c" }),
    ]);
    expect(r.mode).toBe("maia");
    expect(r.spots.map((s) => s.gameId)).toEqual(["a"]);
  });

  it("falls back to critical positions while difficulty is pending, biggest gap first", () => {
    const r = pickHardSpots([
      spot({ gameId: "a", multipvGap: 18 }),
      spot({ gameId: "b", multipvGap: 30 }),
      spot({ gameId: "c", multipvGap: CRITICAL_GAP - 1, isCritical: false }),
      spot({ gameId: "d", multipvGap: 25, uci: "e1g1" }), // not the engine's move
      spot({ gameId: "e", multipvGap: 16 }),
      spot({ gameId: "f", multipvGap: 17 }),
    ]);
    expect(r.mode).toBe("critical");
    expect(r.spots.map((s) => s.gameId)).toEqual(["b", "a", "f"]);
  });

  it("takes one position per game when it can", () => {
    const r = pickHardSpots([
      spot({ gameId: "a", ply: 21, multipvGap: 40 }),
      spot({ gameId: "a", ply: 31, multipvGap: 35 }),
      spot({ gameId: "b", multipvGap: 20 }),
    ]);
    expect(r.spots.map((s) => `${s.gameId}:${s.ply}`)).toEqual(["a:21", "b:21", "a:31"]);
  });

  it("is none when nothing qualifies", () => {
    expect(pickHardSpots([spot({ isCritical: false, multipvGap: 3 })])).toEqual({
      mode: "none",
      spots: [],
    });
    expect(pickHardSpots([spot({ multipvGap: Number.NaN })]).mode).toBe("none");
  });
});

describe("keepDoingNote", () => {
  it("quotes the glossary tip for the strength's kind", () => {
    expect(keepDoingNote("tactic_found")).toBe(
      "Keep doing this: on every move, look at your own checks, captures and threats before anything else.",
    );
    expect(keepDoingNote("only_move_found")).toMatch(/^Keep doing this: when the position is sharp/);
  });
});

describe("reportStrengths", () => {
  it("reads the report input's strengths (Rust select_strengths), dropping malformed ones", () => {
    expect(
      reportStrengths({
        strengths: [
          { area: "tactics", label: "Sharp tactical eye", metricKey: null, exemplars: [{}, {}] },
          { area: "nowhere", label: "Bad area" },
          { area: "endgame", label: "" },
          "nope",
        ],
      }),
    ).toEqual([{ area: "tactics", label: "Sharp tactical eye", metricKey: null, examples: 2 }]);
    expect(reportStrengths(null)).toEqual([]);
    expect(reportStrengths({ strengths: "x" })).toEqual([]);
  });
});
