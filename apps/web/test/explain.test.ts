import { describe, expect, it } from "vitest";
import {
  areaMeaning,
  exampleLine,
  headlineMeaning,
  howOftenText,
  levelText,
  oneInText,
  trendText,
} from "../src/lib/explain";

const BAD = /NaN|undefined|Infinity|null/;

describe("oneInText", () => {
  it.each([
    [14, 40, "about 1 in 3"],
    [1, 40, "about 1 in 40"],
    [22, 120, "about 1 in 5"],
    [58, 2400, "about 1 in 41"],
    [3, 4, "about 8 in 10"],
    [20, 40, "about 5 in 10"],
    [40, 40, "every time"],
    [0, 40, "never"],
    [0, 0, null],
    [Number.NaN, 10, null],
  ])("%s of %s is %s", (events, chances, want) => {
    expect(oneInText(events, chances)).toBe(want);
  });
});

describe("howOftenText", () => {
  const window = "in the last 6 months";
  it.each([
    [
      { events: 14, chances: 40 },
      "In the last 6 months this happened in 14 of your 40 chances (about 1 in 3).",
    ],
    [
      { events: 1, chances: 1 },
      "In the last 6 months this happened in 1 of your 1 chance (every time).",
    ],
    [{ events: 0, chances: 30 }, "In the last 6 months this happened in none of your 30 chances."],
    [
      { events: 0, chances: 0 },
      "In the last 6 months you had no chances for this yet, so there is nothing to count.",
    ],
    [
      { events: 1234, chances: 20000 },
      "In the last 6 months this happened in 1,234 of your 20,000 chances (about 1 in 16).",
    ],
  ])("%o", (counts, want) => {
    const text = howOftenText(counts, window);
    expect(text).toBe(want);
    expect(text).not.toMatch(BAD);
  });
});

describe("trendText", () => {
  const base = {
    state: "active" as const,
    compared: true,
    sufficient: true,
    needMore: null as number | null,
    pImp: null as number | null,
  };
  it.each([
    [
      { ...base, sufficient: false, needMore: 12 },
      "Too early to tell. 12 more chances are needed before it can be called better or worse.",
    ],
    [
      { ...base, sufficient: false, needMore: 1 },
      "Too early to tell. 1 more chance is needed before it can be called better or worse.",
    ],
    [
      { ...base, sufficient: false, needMore: null },
      "Too early to tell. There are not enough chances yet to call it better or worse.",
    ],
    [
      { ...base, compared: false },
      "No verdict yet. Without a benchmark for players at your level, it is not tracked as getting better or worse.",
    ],
    [
      { ...base, state: "improving" as const, pImp: 0.91 },
      "Getting better. Your rate has fallen since its last change, and we are 91% sure it is lower now. It is not settled yet.",
    ],
    [
      { ...base, state: "improving" as const },
      "Getting better. A fall in your rate has been confirmed. It is not settled yet.",
    ],
    [
      { ...base, state: "active" as const, pImp: 0.42 },
      "No clear change yet. There is a 42% chance your rate is lower than before its last change, short of the 85% needed to call it better.",
    ],
    [
      { ...base, state: "active" as const },
      "No clear change. No rise or fall in your rate has been confirmed.",
    ],
    [
      { ...base, state: "emerging" as const },
      "Getting worse. A rise in your rate has been confirmed recently.",
    ],
    [
      { ...base, state: "relapsed" as const },
      "Coming back. It had been fixed, and your rate has been drifting up again.",
    ],
    [
      { ...base, state: "resolved" as const },
      "Fixed for now. Your rate stayed low for 10 analysed games in a row.",
    ],
    [
      { ...base, state: "not_an_issue" as const },
      "Steady. No rise or fall in your rate has been confirmed.",
    ],
  ])("%o", (input, want) => {
    expect(trendText(input)).toBe(want);
  });
});

describe("exampleLine", () => {
  it.each([
    [52.4, 36.1, "Your win chance went from 52% to 36%, a drop of 16 win% points."],
    [88, 12, "Your win chance went from 88% to 12%, a drop of 76 win% points."],
    [50, 50, "Your win chance stayed at 50%."],
    [Number.NaN, 30, null],
  ])("%s to %s", (before, after, want) => {
    expect(exampleLine(before, after)).toBe(want);
  });
});

const est = (value: number, low: number, high: number, n: number) => ({
  kind: "estimate" as const,
  value,
  interval: { low, high, level: 0.8 as const },
  n,
});

describe("headlineMeaning", () => {
  it.each([
    [
      {
        accuracy: { kind: "insufficient" as const, n: 3, needed: 7 },
        blunders: { kind: "insufficient" as const, n: 3, needed: 7 },
        games: "rapid games",
      },
      "Too little evidence yet: with 3 rapid games, one unusual game moves the average a lot.",
    ],
    [
      {
        accuracy: est(74.2, 72.8, 75.6, 40),
        blunders: est(1.1, 0.8, 1.4, 40),
        games: "rapid games",
      },
      "In a typical game you make about 1.1 blunders (moves that cost 15 win% points or more), likely between 0.8 and 1.4. Not compared with players at your level yet.",
    ],
    [
      {
        accuracy: est(80, 78, 82, 40),
        blunders: est(0.25, 0.2, 0.3, 40),
        games: "rapid games",
      },
      "You make a blunder (a move that costs 15 win% points or more) about once every 4 games, likely between 0.2 and 0.3 a game. Not compared with players at your level yet.",
    ],
    [
      {
        accuracy: est(60, 57, 63, 40),
        blunders: est(2.6, 2.2, 3, 40),
        games: "rapid games",
      },
      "In a typical game you make about 2.6 blunders (moves that cost 15 win% points or more), likely between 2.2 and 3. Not compared with players at your level yet.",
    ],
    [
      {
        accuracy: est(80, 78, 82, 40),
        blunders: { kind: "estimate" as const, value: 0, interval: null, n: 40 },
        games: "rapid games",
      },
      "None of your 40 rapid games had a blunder (a move that costs 15 win% points or more). Not compared with players at your level yet.",
    ],
    [
      {
        accuracy: est(74.2, 72.8, 75.6, 40),
        blunders: { kind: "insufficient" as const, n: 40, needed: 0 },
        games: "rapid games",
      },
      "Your accuracy is likely between 72.8% and 75.6% across 40 rapid games. Not compared with players at your level yet.",
    ],
  ])("%o", (input, want) => {
    const text = headlineMeaning(input);
    expect(text).toBe(want);
    expect(text).not.toMatch(BAD);
  });
});

describe("areaMeaning", () => {
  it.each([
    [
      { score: 41, interval: null, n: 15 },
      "41 on a scale where 50 is the typical player in your rating band: 9 points below typical. It has no range yet, so read it as a lean, not a verdict.",
    ],
    [
      { score: 62, interval: { low: 56, high: 68 }, n: 15 },
      "62 on a scale where 50 is the typical player in your rating band: likely between 56 and 68, so above typical.",
    ],
    [
      { score: 41, interval: { low: 35, high: 47 }, n: 15 },
      "41 on a scale where 50 is the typical player in your rating band: likely between 35 and 47, so below typical.",
    ],
    [
      { score: 48, interval: { low: 42, high: 54 }, n: 15 },
      "48 on a scale where 50 is the typical player in your rating band: likely between 42 and 54, so it cannot yet be told apart from typical.",
    ],
    [
      { score: 50, interval: null, n: 15 },
      "50 on a scale where 50 is the typical player in your rating band: level with typical. It has no range yet, so read it as a lean, not a verdict.",
    ],
    [
      { score: 72, interval: null, n: 15 },
      "72 on a scale where 50 is the typical player in your rating band: 22 points above typical. It has no range yet, so read it as a lean, not a verdict.",
    ],
    [
      { score: null, interval: null, n: 6 },
      "Too little evidence for a score yet: its thinnest measure has 6 so far.",
    ],
    [
      { score: null, interval: null, n: 0 },
      "Too little evidence for a score yet: none of its measures has a sample.",
    ],
    [
      { score: Number.NaN, interval: null, n: 6 },
      "Too little evidence for a score yet: its thinnest measure has 6 so far.",
    ],
  ])("%o", (input, want) => {
    const text = areaMeaning(input);
    expect(text).toBe(want);
    expect(text).not.toMatch(BAD);
  });

  it("mindset has no score by design", () => {
    expect(areaMeaning({ score: null, interval: null, n: 30, unscored: true })).toBe(
      "Not scored: session habits do not compare cleanly with other players, so the measures below describe it instead.",
    );
  });
});

describe("levelText", () => {
  const rate = { mean: 0.042, low: 0.031, high: 0.057 };
  it.each([
    [
      { rate, median: 0.021, compared: true, sufficient: true },
      "More often than typical players at your level (2.1%): their rate is below your whole likely range.",
    ],
    [
      { rate, median: 0.08, compared: true, sufficient: true },
      "Less often than typical players at your level (8.0%): their rate is above your whole likely range.",
    ],
    [
      { rate, median: 0.04, compared: true, sufficient: true },
      "Not clearly different from typical players at your level (4.0%): their rate is inside your likely range.",
    ],
    [
      { rate, median: 0.021, compared: true, sufficient: false },
      "Typical players at your level: 2.1%. A comparison waits for more chances.",
    ],
    [
      { rate, median: null, compared: false, sufficient: true },
      "Not compared yet: no benchmark for players at your level yet.",
    ],
    [
      { rate, median: Number.NaN, compared: true, sufficient: true },
      "Not compared yet: no benchmark for players at your level yet.",
    ],
  ])("%o", (input, want) => {
    expect(levelText(input)).toBe(want);
  });
});

describe("levelText with a peer benchmark (ADR 0080)", () => {
  const rate = { mean: 0.042, low: 0.031, high: 0.057 };
  const benchmark = {
    source: "peers" as const,
    platform: "chesscom" as const,
    low: 650,
    high: 950,
    players: 12,
    games: 40,
    chances: {},
  };
  it.each([
    [
      0.021,
      true,
      "More often than players you've faced on chess.com, rated about 650–950 (2.1%): their rate is below your whole likely range.",
    ],
    [
      0.08,
      true,
      "Less often than players you've faced on chess.com, rated about 650–950 (8.0%): their rate is above your whole likely range.",
    ],
    [
      0.04,
      true,
      "Not clearly different from players you've faced on chess.com, rated about 650–950 (4.0%): their rate is inside your likely range.",
    ],
    [
      0.021,
      false,
      "Players you've faced on chess.com, rated about 650–950: 2.1%. A comparison waits for more chances.",
    ],
  ])("median %s, sufficient %s", (median, sufficient, want) => {
    const text = levelText({ rate, median, compared: true, sufficient, benchmark });
    expect(text).toBe(want);
    expect(text).not.toMatch(/players at your level/);
  });

  // ADR 0084 (bug hunt N10): real-shaped rows of the owner's rapid profile.
  const faced = { ...benchmark, low: 667, high: 967 };
  const allowed = { mean: 0.00522, low: 0.00293, high: 0.00782 };
  it("an active row at confidence 0.81 inside the range says probably, with the odds", () => {
    expect(
      levelText({
        rate: allowed,
        median: 0.003498,
        compared: true,
        sufficient: true,
        benchmark: faced,
        confidence: 0.81,
        state: "active",
      }),
    ).toBe(
      "Probably more often than players you've faced on chess.com, rated about 667–967 (0.3%): about 81% likely, though their rate is inside your likely range.",
    );
  });

  it("an active row below confidence 0.7 says where its state comes from", () => {
    expect(
      levelText({
        rate: { mean: 0.4443, low: 0.3819, high: 0.5071 },
        median: 0.4381,
        compared: true,
        sufficient: true,
        benchmark: faced,
        confidence: 0.547,
        state: "active",
      }),
    ).toBe(
      "Not clearly different from players you've faced on chess.com, rated about 667–967 (43.8%): their rate is inside your likely range. Active comes from your game-by-game record (every game since the last change it found), not from this comparison.",
    );
  });

  it("a row that is not a costing state gets no state sentence", () => {
    expect(
      levelText({
        rate,
        median: 0.04,
        compared: true,
        sufficient: true,
        benchmark,
        confidence: 0.4,
        state: "not_an_issue",
      }),
    ).toBe(
      "Not clearly different from players you've faced on chess.com, rated about 650–950 (4.0%): their rate is inside your likely range.",
    );
  });

  it("keeps the band wording for the legacy band reference", () => {
    expect(
      levelText({
        rate,
        median: 0.04,
        compared: true,
        sufficient: true,
        benchmark: { source: "lichess_bands" },
      }),
    ).toBe(
      "Not clearly different from typical players at your level (4.0%): their rate is inside your likely range.",
    );
  });
});
