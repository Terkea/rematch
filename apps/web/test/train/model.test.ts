import { describe, expect, it } from "vitest";
import {
  acceptedSet,
  addDays,
  afterAttempt,
  beforeAfter,
  buildSession,
  type Candidate,
  DAILY_CAP,
  feedback,
  grade,
  isHeldOut,
  keepCandidate,
  type MotifCounts,
  NEW_PER_DAY,
  patternFound,
  primarySource,
  type QueueItem,
  type Schedule,
  SESSION_SIZE,
} from "../../src/server/train/model";

// The missed-moments trainer's pure rules (ADR 0090): the accepted set and its mate band, the
// Leitner schedule, today's session, the selection filters, the holdout and the efficacy rates.

describe("acceptedSet", () => {
  it("is the engine's move alone without stored alternatives", () => {
    expect(acceptedSet("e2e4", [])).toEqual({ accepted: ["e2e4"], mateIn: null });
  });

  it("keeps moves within 5 win% points of the best", () => {
    const lines = [
      { uci: "d1h5", score: { cp: 300 }, pv: ["d1h5"] },
      // 300 cp is about 75.1 win%, 250 cp about 71.4, 200 cp about 67.5.
      { uci: "g1f3", score: { cp: 250 }, pv: ["g1f3"] },
      { uci: "b1c3", score: { cp: 200 }, pv: ["b1c3"] },
    ];
    expect(acceptedSet("d1h5", lines).accepted).toEqual(["d1h5", "g1f3"]);
  });

  it("accepts a mate within best+1 and nothing slower or unforced", () => {
    const lines = [
      { uci: "h5f7", score: { mate: 2 }, pv: [] },
      { uci: "h5h7", score: { mate: 3 }, pv: [] },
      { uci: "h5g6", score: { mate: 4 }, pv: [] },
    ];
    expect(acceptedSet("h5f7", lines)).toEqual({ accepted: ["h5f7", "h5h7"], mateIn: 2 });
    // A winning move that does not mate is not one of the good moves for a missed mate.
    const won = [
      { uci: "h5f7", score: { mate: 1 }, pv: [] },
      { uci: "a1a8", score: { cp: 900 }, pv: [] },
    ];
    expect(acceptedSet("h5f7", won).accepted).toEqual(["h5f7"]);
  });

  it("measures against the top line when the stored best move is not among the lines", () => {
    const lines = [
      { uci: "g1f3", score: { cp: 100 }, pv: [] },
      { uci: "b1c3", score: { cp: 90 }, pv: [] },
    ];
    expect(acceptedSet("e2e4", lines).accepted).toEqual(["e2e4", "g1f3", "b1c3"]);
  });
});

describe("grade", () => {
  it("passes a move of the accepted set, whatever its case", () => {
    expect(grade(["e7e8q", "d1h5"], "E7E8Q")).toBe(true);
    expect(grade(["d1h5"], "d1h5")).toBe(true);
  });

  it("fails anything else", () => {
    expect(grade(["d1h5"], "d1h6")).toBe(false);
    expect(grade(["e7e8q"], "e7e8n")).toBe(false);
  });
});

describe("feedback", () => {
  const fen = "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1";
  it("names a mate in the right answer", () => {
    expect(feedback({ correct: true, fen, moveUci: "d1d8", solutionUci: "d1d8" })).toBe(
      "Right: Rd8# mates.",
    );
  });

  it("names the engine's move when the player's was another good one", () => {
    const open = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    expect(feedback({ correct: true, fen: open, moveUci: "d2d4", solutionUci: "e2e4" })).toBe(
      "Right: d4 is one of the engine's good moves here (it chose e4).",
    );
    expect(feedback({ correct: true, fen: open, moveUci: "e2e4", solutionUci: "e2e4" })).toBe(
      "Right: e4 is the engine's move.",
    );
  });

  it("is about the move, never the player, when it fails", () => {
    expect(feedback({ correct: false, fen, moveUci: "d1d7", solutionUci: "d1d8" })).toBe(
      "Not this time: the engine's move was Rd8#.",
    );
  });
});

describe("afterAttempt", () => {
  const fresh: Schedule = {
    streak: 0,
    cleanPasses: 0,
    dueOn: null,
    introducedOn: null,
    lastAttemptOn: null,
    retired: false,
  };

  it("walks the intervals 1, 3, 7, 21 and 60 days on each right answer", () => {
    let s = fresh;
    let day = "2026-11-01";
    const gaps: number[] = [];
    for (let i = 0; i < 6; i++) {
      s = afterAttempt(s, { correct: true, confidence: "guess", today: day });
      const next = s.dueOn as string;
      gaps.push((Date.parse(next) - Date.parse(day)) / 864e5);
      day = next;
    }
    expect(gaps).toEqual([1, 3, 7, 21, 60, 60]);
    expect(s.introducedOn).toBe("2026-11-01");
  });

  it("resets to 1 day on a miss", () => {
    let s = afterAttempt(fresh, { correct: true, confidence: "sure", today: "2026-11-01" });
    s = afterAttempt(s, { correct: true, confidence: "sure", today: "2026-11-02" });
    s = afterAttempt(s, { correct: false, confidence: "sure", today: "2026-11-05" });
    expect(s).toMatchObject({ streak: 0, cleanPasses: 0, dueOn: "2026-11-06" });
  });

  it("retires after 3 sure right answers at gaps of 7 days or more", () => {
    let s = fresh;
    const days = ["2026-11-01", "2026-11-02", "2026-11-05", "2026-11-12", "2026-12-03"];
    for (const today of days) s = afterAttempt(s, { correct: true, confidence: "sure", today });
    expect(s).toMatchObject({ cleanPasses: 2, retired: false });
    s = afterAttempt(s, { correct: true, confidence: "sure", today: "2027-02-01" });
    expect(s).toMatchObject({ cleanPasses: 3, retired: true });
  });

  it("counts no clean pass for a guess or a gap under 7 days", () => {
    let s = afterAttempt(fresh, { correct: true, confidence: "sure", today: "2026-11-01" });
    s = afterAttempt(s, { correct: true, confidence: "guess", today: "2026-11-09" });
    expect(s.cleanPasses).toBe(0);
    s = afterAttempt(s, { correct: true, confidence: "sure", today: "2026-11-12" });
    expect(s.cleanPasses).toBe(0);
  });
});

describe("buildSession", () => {
  const today = "2026-11-23";
  const item = (i: number, over: Partial<QueueItem> = {}): QueueItem => ({
    id: `item-${String(i).padStart(3, "0")}`,
    sourceKey: "blunder",
    sourceKeys: ["blunder"],
    dueOn: null,
    introducedOn: null,
    heldOut: false,
    retired: false,
    playedAt: new Date(Date.UTC(2026, 9, 1) + i * 864e5),
    ...over,
  });
  const base = { today, attemptedToday: 0, introducedToday: 0, seed: "u1" };

  it("serves due items first, then new ones up to the session size", () => {
    const due = Array.from({ length: 5 }, (_, i) =>
      item(i, { dueOn: "2026-11-20", introducedOn: "2026-11-01" }),
    );
    const later = item(50, { dueOn: "2026-11-24", introducedOn: "2026-11-01" });
    const fresh = Array.from({ length: 20 }, (_, i) => item(100 + i));
    const ids = buildSession([...due, later, ...fresh], base);
    expect(ids).toHaveLength(SESSION_SIZE);
    expect(ids).not.toContain(later.id);
    for (const d of due) expect(ids).toContain(d.id);
  });

  it("seeds at most NEW_PER_DAY new items a day", () => {
    const fresh = Array.from({ length: 30 }, (_, i) => item(i));
    expect(buildSession(fresh, base)).toHaveLength(Math.min(NEW_PER_DAY, SESSION_SIZE));
    expect(buildSession(fresh, { ...base, introducedToday: NEW_PER_DAY - 2 })).toHaveLength(2);
  });

  it("stops at the daily cap", () => {
    const due = Array.from({ length: 30 }, (_, i) =>
      item(i, { dueOn: "2026-11-01", introducedOn: "2026-10-01" }),
    );
    expect(buildSession(due, { ...base, attemptedToday: DAILY_CAP - 3 })).toHaveLength(3);
    expect(buildSession(due, { ...base, attemptedToday: DAILY_CAP })).toEqual([]);
  });

  it("never serves a held-out or retired item", () => {
    const ids = buildSession(
      [item(1, { heldOut: true }), item(2, { retired: true }), item(3)],
      base,
    );
    expect(ids).toEqual(["item-003"]);
  });

  it("mixes the sources rather than blocking them", () => {
    const mates = Array.from({ length: 4 }, (_, i) =>
      item(i, { sourceKey: "W4.missed", sourceKeys: ["W4.missed", "blunder"] }),
    );
    const blunders = Array.from({ length: 4 }, (_, i) => item(10 + i));
    const ids = buildSession([...mates, ...blunders], base);
    const kinds = ids.map((id) => (mates.some((m) => m.id === id) ? "m" : "b"));
    for (let i = 1; i < kinds.length; i++) expect(kinds[i]).not.toBe(kinds[i - 1]);
  });

  it("filters to one source, matching any source an item belongs to", () => {
    const mate = item(1, { sourceKey: "W4.missed", sourceKeys: ["W4.missed", "blunder"] });
    const ids = buildSession([mate, item(2)], { ...base, source: "W4.missed" });
    expect(ids).toEqual([mate.id]);
    expect(buildSession([mate, item(2)], { ...base, source: "blunder" })).toHaveLength(2);
  });
});

describe("selection filters", () => {
  const c = (over: Partial<Candidate> = {}): Candidate => ({
    gameId: "g1",
    ply: 21,
    // White to move: Rd8# mates; the rook takes nothing.
    fen: "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1",
    uci: "d1d7",
    bestUci: "d1d8",
    winBefore: 99,
    winAfter: 60,
    prevSan: "h6",
    sources: ["W4.missed"],
    ...over,
  });

  it("keeps a missed mate", () => {
    expect(keepCandidate(c())).toBe(true);
  });

  it("drops a position already decided before and after the move", () => {
    expect(keepCandidate(c({ winBefore: 98, winAfter: 92, sources: ["blunder"] }))).toBe(false);
    expect(keepCandidate(c({ winBefore: 98, winAfter: 80, sources: ["blunder"] }))).toBe(true);
  });

  it("drops a free capture that is not a mate", () => {
    // The c3 knight takes the undefended queen on d5 for free.
    const fen = "4k3/8/8/3q4/8/2N5/8/4K3 w - - 0 1";
    expect(keepCandidate(c({ fen, uci: "e1e2", bestUci: "c3d5", sources: ["blunder"] }))).toBe(
      false,
    );
  });

  it("needs an engine move other than the one played", () => {
    expect(keepCandidate(c({ bestUci: null }))).toBe(false);
    expect(keepCandidate(c({ bestUci: "d1d7" }))).toBe(false);
    expect(keepCandidate(c({ sources: [] }))).toBe(false);
  });

  it("names the most specific source", () => {
    expect(primarySource(["blunder", "W7.traps"])).toBe("W7.traps");
    expect(primarySource(["blunder", "W4.missed", "W3.shallow"])).toBe("W4.missed");
    expect(primarySource(["blunder"])).toBe("blunder");
  });
});

describe("isHeldOut", () => {
  it("holds out about 20% of positions, the same ones every time", () => {
    const keys = Array.from({ length: 2000 }, (_, i) => [`game-${i}`, (i % 60) + 1] as const);
    const held = keys.filter(([g, p]) => isHeldOut(g, p)).length / keys.length;
    expect(held).toBeGreaterThan(0.17);
    expect(held).toBeLessThan(0.23);
    expect(isHeldOut("game-7", 8)).toBe(isHeldOut("game-7", 8));
  });
});

describe("beforeAfter", () => {
  const g = (day: number, events: number, chances: number) => ({
    playedAt: new Date(Date.UTC(2026, 9, 1) + day * 864e5),
    events,
    chances,
  });
  const drill = new Date(Date.UTC(2026, 9, 11));

  it("says too early below 30 chances after the first drill", () => {
    const r = beforeAfter([g(1, 5, 40), g(12, 1, 10)], drill);
    expect(r.before).toMatchObject({ events: 5, chances: 40 });
    expect(r.after).toMatchObject({ events: 1, chances: 10 });
    expect(r.verdict).toBe("too_early");
  });

  it("says lower only when the 80% intervals separate", () => {
    const r = beforeAfter([g(1, 40, 100), g(12, 5, 100)], drill);
    expect(r.verdict).toBe("lower");
    expect(r.after.rate).toBeCloseTo(0.05);
    expect((r.after.high ?? 1) < (r.before.low ?? 0)).toBe(true);
    expect(beforeAfter([g(1, 20, 100), g(12, 17, 100)], drill).verdict).toBe("no_clear_change");
  });

  it("has no after side before any drill", () => {
    const r = beforeAfter([g(1, 5, 40)], null);
    expect(r.after).toMatchObject({ chances: 0, rate: null });
    expect(r.verdict).toBe("too_early");
  });
});

describe("patternFound", () => {
  it("counts the chances at the item's motifs in games after it, and how often they were found", () => {
    const games: { playedAt: Date; byMotif: MotifCounts }[] = [
      { playedAt: new Date("2026-10-01"), byMotif: { fork: { opportunities: 9, misses: 9 } } },
      {
        playedAt: new Date("2026-10-20"),
        byMotif: { fork: { opportunities: 4, misses: 1 }, pin: { opportunities: 2, misses: 0 } },
      },
    ];
    const r = patternFound(games, new Set(["fork"]), new Date("2026-10-10"));
    expect(r).toMatchObject({ events: 3, chances: 4 });
    expect(r.rate).toBeCloseTo(0.75);
  });
});

describe("addDays", () => {
  it("adds whole UTC days", () => {
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
  });
});
