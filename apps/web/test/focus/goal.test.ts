import { describe, expect, it } from "vitest";
import {
  chooseTarget,
  focusProgress,
  MIN_VERDICT_CHANCES,
  RELATIVE_STEP,
} from "../../src/server/focus/goal";

describe("chooseTarget", () => {
  it("takes 25% off the current rate when the peer rate is harder to reach", () => {
    // 20% now, peers at 10%: -25% (15%) is the easier of the two.
    expect(chooseTarget({ baseline: 0.2, peer: 0.1 })).toEqual({ rate: 0.15, basis: "relative" });
  });

  it("takes the peer rate when it is the easier step", () => {
    // 14.6% now, peers at 12%: 12% is easier than 10.95%.
    expect(chooseTarget({ baseline: 0.146, peer: 0.12 })).toEqual({ rate: 0.12, basis: "peer" });
  });

  it("ignores a peer rate that is no meaningful step down", () => {
    // Peers at 13.5% are barely below 14%: the relative step stands.
    expect(chooseTarget({ baseline: 0.14, peer: 0.135 })).toEqual({
      rate: 0.105,
      basis: "relative",
    });
    expect(chooseTarget({ baseline: 0.1, peer: 0.2 })).toEqual({ rate: 0.075, basis: "relative" });
  });

  it("uses the relative step without a peer rate", () => {
    expect(chooseTarget({ baseline: 0.08, peer: null })).toEqual({ rate: 0.06, basis: "relative" });
    expect(RELATIVE_STEP).toBe(0.25);
  });

  it("rounds to a tenth of a percentage point", () => {
    expect(chooseTarget({ baseline: 0.1234, peer: null })?.rate).toBe(0.093);
  });

  it("has no target for a rate that cannot go down", () => {
    expect(chooseTarget({ baseline: 0, peer: 0.1 })).toBeNull();
    expect(chooseTarget({ baseline: Number.NaN, peer: null })).toBeNull();
  });
});

describe("focusProgress", () => {
  const base = { gamesN: 20, target: 0.1 };

  it("is too early to tell below the minimum chances, saying how many more", () => {
    const p = focusProgress({ ...base, k: 1, n: 4, games: 6 });
    expect(p.verdict).toEqual({ kind: "too_early", moreChances: MIN_VERDICT_CHANCES - 4 });
    expect(p.rate).toBe(0.25);
    expect(p.interval).not.toBeNull();
    expect(p.done).toBe(false);
    expect(p.outcome).toBeNull();
  });

  it("has no rate and no interval with no chance yet", () => {
    const p = focusProgress({ ...base, k: 0, n: 0, games: 0 });
    expect(p.rate).toBeNull();
    expect(p.interval).toBeNull();
    expect(p.verdict).toEqual({ kind: "too_early", moreChances: MIN_VERDICT_CHANCES });
  });

  it("is on track at or below the target with enough chances", () => {
    const p = focusProgress({ ...base, k: 1, n: 12, games: 9 });
    expect(p.verdict).toEqual({ kind: "on_track" });
    expect(p.interval?.low).toBeLessThan(p.rate ?? 0);
    expect(p.interval?.high).toBeGreaterThan(p.rate ?? 1);
  });

  it("is not yet above the target with enough chances", () => {
    expect(focusProgress({ ...base, k: 3, n: 12, games: 9 }).verdict).toEqual({ kind: "not_yet" });
  });

  it("closes at N games: met, not met or inconclusive", () => {
    const met = focusProgress({ ...base, k: 1, n: 15, games: 20 });
    expect(met).toMatchObject({ done: true, outcome: "met" });
    const missed = focusProgress({ ...base, k: 4, n: 15, games: 20 });
    expect(missed).toMatchObject({ done: true, outcome: "not_met" });
    const thin = focusProgress({ ...base, k: 0, n: 6, games: 20 });
    expect(thin).toMatchObject({ done: true, outcome: "inconclusive" });
  });

  it("counts games played out of N, never past N", () => {
    expect(focusProgress({ ...base, k: 0, n: 0, games: 7 }).games).toEqual({
      played: 7,
      of: 20,
    });
    expect(focusProgress({ ...base, k: 0, n: 0, games: 25 }).games.played).toBe(20);
  });
});
