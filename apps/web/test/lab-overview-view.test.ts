import { describe, expect, it } from "vitest";
import {
  accountText,
  columnGames,
  estimateView,
  latest,
  splitRows,
  statusText,
  trendPoints,
} from "../src/app/lab/overview/view.js";
import type { Overview } from "../src/server/overview/types.js";

const day = (d: string) => new Date(`${d}T12:00:00Z`);

describe("statusText", () => {
  it("says only the analysed count when everything is done", () => {
    expect(statusText({ total: 500, analysed: 500, waiting: 0, failed: 0, skipped: 0 })).toEqual([
      "500 games analysed",
    ]);
  });
  it("names waiting, failed and skipped games when there are any", () => {
    expect(statusText({ total: 500, analysed: 412, waiting: 83, failed: 3, skipped: 2 })).toEqual([
      "412 of 500 games analysed.",
      "83 waiting for analysis.",
      "3 games couldn't be analysed.",
      "2 games skipped (variants or too short).",
    ]);
  });
  it("uses singular forms", () => {
    expect(statusText({ total: 2, analysed: 1, waiting: 1, failed: 0, skipped: 0 })).toEqual([
      "1 of 2 games analysed.",
      "1 waiting for analysis.",
    ]);
    expect(statusText({ total: 1, analysed: 1, waiting: 0, failed: 0, skipped: 0 })).toEqual([
      "1 game analysed",
    ]);
    expect(
      statusText({ total: 3, analysed: 1, waiting: 0, failed: 1, skipped: 1 }).slice(1),
    ).toEqual(["1 game couldn't be analysed.", "1 game skipped (variants or too short)."]);
  });
});

describe("accountText", () => {
  const base: Overview["accounts"][number] = {
    id: "a",
    platform: "chesscom",
    username: "someplayer",
    displayName: "SomePlayer",
    verified: true,
    lastSyncedAt: null,
    lastImport: null,
  };
  it("names the platform and the username", () => {
    expect(accountText(base)).toEqual({ name: "chess.com someplayer", notes: [] });
  });
  it("says Unverified and the last import day when known", () => {
    expect(
      accountText({
        ...base,
        platform: "lichess",
        verified: false,
        lastImport: { status: "done", imported: 500, finishedAt: day("2026-11-21") },
      }),
    ).toEqual({ name: "Lichess someplayer", notes: ["Unverified", "Last imported 21 Nov 2026"] });
  });
});

describe("estimateView", () => {
  it("asks for more games below the minimum, with no number", () => {
    expect(estimateView({ kind: "insufficient", n: 3, needed: 7 }, "percent", "rapid")).toEqual({
      kind: "insufficient",
      need: "Need 7 more games",
      basis: "Based on 3 rapid games",
    });
    expect(estimateView({ kind: "insufficient", n: 9, needed: 1 }, "decimal", "analysed")).toEqual({
      kind: "insufficient",
      need: "Need 1 more game",
      basis: "Based on 9 analysed games",
    });
  });
  it("divides percent values and their interval by 100", () => {
    expect(
      estimateView(
        { kind: "estimate", value: 58.4, interval: { low: 56, high: 60.6, level: 0.8 }, n: 120 },
        "percent",
        "analysed",
      ),
    ).toEqual({
      kind: "kpi",
      value: expect.closeTo(0.584, 12),
      interval: { low: expect.closeTo(0.56, 12), high: expect.closeTo(0.606, 12), level: 0.8 },
      basis: { games: 120, timeClass: "analysed" },
    });
  });
  it("keeps rates (already 0 to 1) and decimals as they are", () => {
    const rate = estimateView(
      { kind: "estimate", value: 0.5, interval: { low: 0.4, high: 0.6, level: 0.8 }, n: 40 },
      "rate",
      "blitz",
    );
    expect(rate).toMatchObject({ kind: "kpi", value: 0.5, interval: { low: 0.4, high: 0.6 } });
    const dec = estimateView(
      { kind: "estimate", value: 2.41, interval: { low: 2.2, high: 2.7, level: 0.8 }, n: 120 },
      "decimal",
      "analysed",
    );
    expect(dec).toMatchObject({ kind: "kpi", value: 2.41, interval: { low: 2.2, high: 2.7 } });
  });
  it("shows a value with no band when the interval cannot be estimated", () => {
    expect(
      estimateView({ kind: "estimate", value: 0, interval: null, n: 12 }, "decimal", "rapid"),
    ).toEqual({
      kind: "flat",
      value: 0,
      note: "Every game gave the same value, so there is no interval",
      basis: { games: 12, timeClass: "rapid" },
    });
  });
});

describe("chart data", () => {
  const points: Overview["accuracy"]["points"] = [
    { id: "g1", playedAt: day("2026-11-10"), accuracy: 71.34, result: "win" },
    { id: "g2", playedAt: day("2026-11-12"), accuracy: 48, result: "loss" },
    { id: "g3", playedAt: day("2026-11-13"), accuracy: 60, result: "draw" },
  ];
  it("captions each trend dot with its day, result and accuracy, and links its review", () => {
    expect(trendPoints(points)[0]).toEqual({
      id: "g1",
      date: "2026-11-10T12:00:00.000Z",
      accuracy: 71.34,
      result: "win",
      caption: "10 Nov 2026, win, accuracy 71.3%",
      href: "/app/games/g1",
    });
  });
  it("keeps the latest games and their rolling entries only", () => {
    const rolling = [
      { id: "g2", mean: 60, low: 50, high: 70 },
      { id: "g3", mean: 59, low: null, high: null },
    ];
    expect(latest(points, rolling, 2)).toEqual({
      points: [points[1], points[2]],
      rolling: [rolling[0], rolling[1]],
    });
    expect(latest(points, rolling, 1)).toEqual({ points: [points[2]], rolling: [rolling[1]] });
    expect(latest(points, rolling, 10).points).toHaveLength(3);
  });
  it("labels judgement columns by day and links their review", () => {
    expect(
      columnGames([
        { id: "g1", playedAt: day("2026-11-10"), blunders: 2, mistakes: 1, inaccuracies: 0 },
      ]),
    ).toEqual([
      {
        id: "g1",
        label: "Game of 10 Nov 2026",
        href: "/app/games/g1",
        blunders: 2,
        mistakes: 1,
        inaccuracies: 0,
      },
    ]);
  });
  it("names the result rows by time control", () => {
    expect(
      splitRows([
        {
          timeClass: "rapid",
          games: 14,
          wins: 8,
          draws: 2,
          losses: 4,
          score: { kind: "insufficient", n: 0, needed: 10 },
          accuracy: { kind: "insufficient", n: 0, needed: 10 },
        },
      ]),
    ).toEqual([{ key: "rapid", label: "Rapid", wins: 8, draws: 2, losses: 4 }]);
  });
});
