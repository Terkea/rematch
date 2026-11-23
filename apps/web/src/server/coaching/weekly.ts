import type { Db } from "@ci/db";
import { weeklyReviews } from "@ci/db/generated";
import { and, desc, eq } from "drizzle-orm";
import { type FocusWeekLine, focusWeekLinesIn } from "../focus";
import { readOnly, scopedAccountIds } from "../games";
import type { AiDisclosure, AiItemStatus, CoachTarget, WeeklyContent } from "./types";

export type WeeklyReviewView = {
  id: string;
  weekStart: string;
  status: AiItemStatus;
  content: WeeklyContent | null;
  droppedCount: number;
  aiDisclosure: AiDisclosure | null;
  /**
   * "Your focus this week" (ADR 0091): computed from the week's games of the focus that ran in
   * it, never written by the model; null for a week with no focus or none of its games.
   */
  focus: FocusWeekLine | null;
};

/**
 * The scope's weekly reviews, newest week first. Content and disclosure are shown only for a
 * `ready` review (Rust `ValidatedWeeklyReview`); `droppedCount` is how many parts the validator
 * dropped. A foreign or malformed account sees none.
 */
export async function listWeeklyReviews(
  db: Db,
  userId: string,
  scope: CoachTarget,
  limit = 12,
): Promise<WeeklyReviewView[]> {
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, scope.accountId);
    if (ids.length === 0) return [];
    const rows = await tx
      .select()
      .from(weeklyReviews)
      .where(
        and(
          eq(weeklyReviews.accountId, scope.accountId),
          eq(weeklyReviews.timeClass, scope.timeClass),
        ),
      )
      .orderBy(desc(weeklyReviews.weekStart))
      .limit(limit);
    const lines = await focusWeekLinesIn(
      tx,
      userId,
      scope,
      rows.map((r) => r.weekStart),
    );
    return rows.map((r) => {
      const ready = r.status === "ready" && r.content != null;
      return {
        id: r.id,
        weekStart: r.weekStart,
        status: r.status,
        content: ready ? (r.content as WeeklyContent) : null,
        droppedCount: Array.isArray(r.dropped) ? r.dropped.length : 0,
        aiDisclosure: ready ? ((r.aiDisclosure as AiDisclosure | null) ?? null) : null,
        focus: lines.get(r.weekStart) ?? null,
      };
    });
  });
}
