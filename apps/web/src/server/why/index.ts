import type { Db } from "@ci/db";
import { drillItems, focuses, weaknessEstimates } from "@ci/db/generated";
import { and, desc, eq, gt, isNull } from "drizzle-orm";
import { type AlgorithmLink, algorithmLink } from "../algorithm";
import type { TimeClass } from "../games/types";
import { MODEL_BASELINE_KEYS, MODEL_BASELINE_LABEL } from "../profile/benchmark-text";
import { seriesLabel } from "../profile/catalogue";
import type { CoachScope } from "../profile/types";

// "Why this?" (ADR 0104 §9, ADR 0105, issue #56): for every recommendation, the facts behind it,
// the rule that chose it, and the algorithm that computed it. Read from the stamped rows only;
// nothing here computes a new number. The disclosure UI is v0.5's.

/** The ranking floor of ADR 0101 (`RANK_MIN_EXCESS_PER_GAME`, `profile-core` `priority.rs`). */
export const RANK_RULE =
  "Ranked only when its 80% range lies wholly above the comparison and it costs at least 0.02 points a game, over at least 30 analysed games (ADR 0101).";
export const NOT_RANKED = "Not ranked: not enough evidence yet (ADR 0101).";

export type WhyFacts = {
  /** Times it happened, and the chances it could have (unweighted counts). */
  events: number;
  chances: number;
  /** The estimated rate with its 80% range. */
  rate: { mean: number; low: number; high: number; level: 0.8 } | null;
  /** What it is compared with: the rating band's peers, or what the evaluation predicts. */
  comparison: { group: string; rate: number | null } | null;
  /** The points it costs a game (the stored `excess`), null when not compared. */
  pointsLostPerGame: number | null;
};

export type WhyThis = {
  kind: "weakness" | "plan" | "focus" | "drill";
  seriesKey: string;
  label: string;
  facts: WhyFacts;
  rule: { ranked: boolean; text: string };
  /** The plan's order, for a plan item (1 is first). */
  rank?: number;
  /** The focus's target and how it was chosen. */
  target?: { rate: number; basis: "peer" | "relative"; gamesN: number };
  /** A drill's position. */
  position?: { gameId: string; ply: number; winBefore: number; winAfter: number };
  algorithm: AlgorithmLink;
};

/** The stored estimate columns "Why this?" reads (`weakness_estimates`). */
export type EstimateFacts = {
  seriesKey: string;
  kUnweighted: number;
  nUnweighted: number;
  mean: number;
  ciLow: number;
  ciHigh: number;
  bandMedian: number | null;
  excess: number | null;
  priority: number;
  algorithmId: string | null;
};

const group = (seriesKey: string): string =>
  MODEL_BASELINE_KEYS.has(seriesKey) ? MODEL_BASELINE_LABEL : "Players at your rating";

function factsOf(e: EstimateFacts): WhyFacts {
  const compared = e.bandMedian !== null;
  return {
    events: e.kUnweighted,
    chances: e.nUnweighted,
    rate: { mean: e.mean, low: e.ciLow, high: e.ciHigh, level: 0.8 },
    comparison: compared ? { group: group(e.seriesKey), rate: e.bandMedian } : null,
    pointsLostPerGame: compared ? e.excess : null,
  };
}

/** A weakness: ranked when its stored priority is above 0 (the floor already applied in Rust). */
export function whyWeakness(e: EstimateFacts): WhyThis {
  const ranked = e.priority > 0;
  return {
    kind: "weakness",
    seriesKey: e.seriesKey,
    label: seriesLabel(e.seriesKey),
    facts: factsOf(e),
    rule: { ranked, text: ranked ? RANK_RULE : NOT_RANKED },
    algorithm: algorithmLink(e.algorithmId),
  };
}

/** The plan's items: the ranked weaknesses in priority order (unranked ones are not in it). */
export function whyPlan(estimates: readonly EstimateFacts[]): WhyThis[] {
  return [...estimates]
    .filter((e) => e.priority > 0)
    .sort((a, b) => b.priority - a.priority || a.seriesKey.localeCompare(b.seriesKey))
    .map((e, i) => ({ ...whyWeakness(e), kind: "plan" as const, rank: i + 1 }));
}

/** The stored focus columns (`focuses`). */
export type FocusFacts = {
  seriesKey: string;
  targetRate: number;
  gamesN: number;
  baselineRate: number;
  baselineK: number;
  baselineN: number;
  peerRate: number | null;
  algorithmId: string | null;
};

/**
 * The weekly focus: its baseline when it was set, and its target. A target equal to the stored
 * peer rate was the peer rate; otherwise it was the relative step (`RELATIVE_STEP`, ADR 0091).
 */
export function whyFocus(f: FocusFacts): WhyThis {
  const basis = f.peerRate !== null && f.targetRate === f.peerRate ? "peer" : "relative";
  return {
    kind: "focus",
    seriesKey: f.seriesKey,
    label: seriesLabel(f.seriesKey),
    facts: {
      events: f.baselineK,
      chances: f.baselineN,
      rate: null,
      comparison: f.peerRate === null ? null : { group: group(f.seriesKey), rate: f.peerRate },
      pointsLostPerGame: null,
    },
    rule: {
      ranked: true,
      text:
        basis === "peer"
          ? `Target: the rate of players at your rating, over your next ${f.gamesN} games (ADR 0091).`
          : `Target: a quarter fewer than your rate when you set it, over your next ${f.gamesN} games (ADR 0091).`,
    },
    target: { rate: f.targetRate, basis, gamesN: f.gamesN },
    algorithm: algorithmLink(f.algorithmId),
  };
}

/** The stored drill columns (`drill_items`). */
export type DrillFacts = {
  sourceKey: string;
  gameId: string;
  ply: number;
  winBefore: number;
  winAfter: number;
  algorithmId: string | null;
};

/** A drill: a position from your own game where the move cost the win share shown. */
export function whyDrill(d: DrillFacts): WhyThis {
  return {
    kind: "drill",
    seriesKey: d.sourceKey,
    label: seriesLabel(d.sourceKey),
    facts: { events: 1, chances: 1, rate: null, comparison: null, pointsLostPerGame: null },
    rule: {
      ranked: true,
      text: "A position from your own game where your move gave away the win share shown; positions already decided are left out (ADR 0090).",
    },
    position: { gameId: d.gameId, ply: d.ply, winBefore: d.winBefore, winAfter: d.winAfter },
    algorithm: algorithmLink(d.algorithmId),
  };
}

// ---- Reads ----

const estimateColumns = {
  seriesKey: weaknessEstimates.seriesKey,
  kUnweighted: weaknessEstimates.kUnweighted,
  nUnweighted: weaknessEstimates.nUnweighted,
  mean: weaknessEstimates.mean,
  ciLow: weaknessEstimates.ciLow,
  ciHigh: weaknessEstimates.ciHigh,
  bandMedian: weaknessEstimates.bandMedian,
  excess: weaknessEstimates.excess,
  priority: weaknessEstimates.priority,
  algorithmId: weaknessEstimates.algorithmId,
};

const scopeOf = (scope: CoachScope) =>
  and(
    eq(weaknessEstimates.accountId, scope.accountId),
    eq(weaknessEstimates.timeClass, scope.timeClass),
    eq(weaknessEstimates.side, scope.side ?? "both"),
  );

/** The top weakness of a scope, or null when nothing is ranked. */
export async function whyTopWeakness(db: Db, scope: CoachScope): Promise<WhyThis | null> {
  const [row] = await db
    .select(estimateColumns)
    .from(weaknessEstimates)
    .where(and(scopeOf(scope), gt(weaknessEstimates.priority, 0)))
    .orderBy(desc(weaknessEstimates.priority), weaknessEstimates.seriesKey)
    .limit(1);
  return row ? whyWeakness(row as EstimateFacts) : null;
}

/** One weakness of a scope, ranked or not; null when the scope has no such estimate. */
export async function whyWeaknessOf(
  db: Db,
  scope: CoachScope,
  seriesKey: string,
): Promise<WhyThis | null> {
  const [row] = await db
    .select(estimateColumns)
    .from(weaknessEstimates)
    .where(and(scopeOf(scope), eq(weaknessEstimates.seriesKey, seriesKey)))
    .limit(1);
  return row ? whyWeakness(row as EstimateFacts) : null;
}

/** The plan's items for a scope, in order. */
export async function whyPlanOf(db: Db, scope: CoachScope): Promise<WhyThis[]> {
  const rows = await db.select(estimateColumns).from(weaknessEstimates).where(scopeOf(scope));
  return whyPlan(rows as EstimateFacts[]);
}

/** The user's running focus for a time class, or null. */
export async function whyFocusOf(db: Db, userId: string, tc: TimeClass): Promise<WhyThis | null> {
  const [row] = await db
    .select({
      seriesKey: focuses.seriesKey,
      targetRate: focuses.targetRate,
      gamesN: focuses.gamesN,
      baselineRate: focuses.baselineRate,
      baselineK: focuses.baselineK,
      baselineN: focuses.baselineN,
      peerRate: focuses.peerRate,
      algorithmId: focuses.algorithmId,
    })
    .from(focuses)
    .where(and(eq(focuses.userId, userId), eq(focuses.timeClass, tc), isNull(focuses.endedAt)))
    .limit(1);
  return row ? whyFocus(row as FocusFacts) : null;
}

/** One of the user's drill items, or null when it is not theirs. */
export async function whyDrillOf(db: Db, userId: string, itemId: string): Promise<WhyThis | null> {
  const [row] = await db
    .select({
      sourceKey: drillItems.sourceKey,
      gameId: drillItems.gameId,
      ply: drillItems.ply,
      winBefore: drillItems.winBefore,
      winAfter: drillItems.winAfter,
      algorithmId: drillItems.algorithmId,
    })
    .from(drillItems)
    .where(and(eq(drillItems.id, itemId), eq(drillItems.userId, userId)))
    .limit(1);
  return row ? whyDrill(row as DrillFacts) : null;
}
