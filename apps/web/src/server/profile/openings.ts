import type { Db } from "@ci/db";
import { openingLines } from "@ci/db/generated";
import { and, eq, isNotNull } from "drizzle-orm";
import { type LineEvidence, lineEvidenceKey } from "../../lib/explain";
import { readOnly } from "../games";
import { ownsScope } from "./scope";
import type { CoachScope } from "./types";

const VERDICTS = new Set(["keep", "fix", "learn", "replace"]);

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const num = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);
const whole = (v: unknown): number => {
  const n = num(v);
  return n !== null && n >= 0 ? Math.round(n) : 0;
};

/**
 * A stored `opening_lines.summary` (Rust `LineSummary`, camelCase) as verdict evidence; null
 * unless it has a verdict and a game count. A malformed field reads as unknown (null), never
 * as a guess.
 */
export function lineEvidenceOf(summary: unknown): LineEvidence | null {
  if (!isObj(summary)) return null;
  const games = num(summary.games);
  const verdict = summary.verdict;
  if (games === null || games < 0 || typeof verdict !== "string" || !VERDICTS.has(verdict)) {
    return null;
  }
  const iv = summary.perfInterval;
  const low = isObj(iv) ? num(iv.low) : null;
  const high = isObj(iv) ? num(iv.high) : null;
  const spot = summary.errorSpot;
  const spotGames = isObj(spot) ? num(spot.games) : null;
  return {
    verdict: verdict as LineEvidence["verdict"],
    color: summary.color === "black" ? "black" : "white",
    kind: summary.kind === "faced" ? "faced" : "chosen",
    games: Math.round(games),
    perfVsExpected: num(summary.perfVsExpected),
    perfGames: whole(summary.perfGames),
    perfInterval: low !== null && high !== null ? { low, high } : null,
    errorSpot:
      isObj(spot) && typeof spot.san === "string" && spot.san !== "" && spotGames !== null
        ? { san: spot.san, games: Math.round(spotGames) }
        : null,
    analysedGames: whole(summary.analysedGames),
    avgWinAtOpeningEnd: num(summary.avgWinAtOpeningEnd),
    medianDeviationPly: num(summary.medianDeviationPly),
    share: num(summary.share) ?? 0,
    struggles: Array.isArray(summary.struggles)
      ? summary.struggles.filter((s): s is string => typeof s === "string")
      : [],
  };
}

/** The scope's stored lines with a verdict, as evidence by `lineEvidenceKey`. */
export async function lineEvidence(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<Record<string, LineEvidence>> {
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return {};
    const rows = await tx
      .select({
        lineKey: openingLines.lineKey,
        color: openingLines.color,
        kind: openingLines.kind,
        summary: openingLines.summary,
      })
      .from(openingLines)
      .where(
        and(
          eq(openingLines.accountId, scope.accountId),
          eq(openingLines.timeClass, scope.timeClass),
          isNotNull(openingLines.verdict),
        ),
      );
    const out: Record<string, LineEvidence> = {};
    for (const r of rows) {
      const e = lineEvidenceOf(r.summary);
      if (e) out[lineEvidenceKey(r.lineKey, r.color, r.kind)] = e;
    }
    return out;
  });
}
