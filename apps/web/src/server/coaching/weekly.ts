import type { Db } from "@ci/db";
import { weeklyReviews } from "@ci/db/generated";
import { and, desc, eq } from "drizzle-orm";
import { isVerifiedLink } from "../accounts/verified";
import { readOnly, scopedAccountIds } from "../games";
import type { AiDisclosure, AiItemStatus, CoachTarget, WeeklyContent } from "./types";

export type WeeklyReviewView = {
  id: string;
  weekStart: string;
  status: AiItemStatus;
  content: WeeklyContent | null;
  droppedCount: number;
  aiDisclosure: AiDisclosure | null;
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
    // Holistic F10: AI text is read only through a verified link.
    if (!(await isVerifiedLink(tx, userId, scope.accountId))) return [];
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
    return rows.map((r) => {
      const ready = r.status === "ready" && r.content != null;
      return {
        id: r.id,
        weekStart: r.weekStart,
        status: r.status,
        content: ready ? (r.content as WeeklyContent) : null,
        droppedCount: Array.isArray(r.dropped) ? r.dropped.length : 0,
        aiDisclosure: ready ? ((r.aiDisclosure as AiDisclosure | null) ?? null) : null,
      };
    });
  });
}
