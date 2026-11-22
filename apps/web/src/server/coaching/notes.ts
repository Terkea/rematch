import type { Db } from "@ci/db";
import { weaknessChangeNotes } from "@ci/db/generated";
import { and, desc, eq, inArray, type SQL, sql } from "drizzle-orm";
import { readOnly, scopedAccountIds } from "../games";
import type { AiDisclosure, AiItemStatus, Citation, NarrativeItem, Segment } from "./types";

export type ChangeNoteView = {
  changeKey: string;
  seriesKey: string;
  onsetDate: string | null;
  status: AiItemStatus;
  item: NarrativeItem | null;
  aiDisclosure: AiDisclosure | null;
};

/**
 * The account's weakness change notes (`explain-weakness-change`), newest onset first, narrowed
 * to `seriesKeys` when given. `item` and the disclosure are set only for a `ready` note with
 * segments. A foreign or malformed account sees none.
 */
export async function changeNotes(
  db: Db,
  userId: string,
  accountId: string,
  seriesKeys?: string[],
): Promise<ChangeNoteView[]> {
  if (seriesKeys && seriesKeys.length === 0) return [];
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId, accountId);
    if (ids.length === 0) return [];
    const where: SQL[] = [eq(weaknessChangeNotes.accountId, accountId)];
    if (seriesKeys) where.push(inArray(weaknessChangeNotes.seriesKey, seriesKeys));
    const rows = await tx
      .select()
      .from(weaknessChangeNotes)
      .where(and(...where))
      .orderBy(
        sql`${weaknessChangeNotes.onsetDate} desc nulls last`,
        desc(weaknessChangeNotes.createdAt),
      );
    return rows.map((r) => {
      const ready = r.status === "ready" && r.text != null && Array.isArray(r.segments);
      return {
        changeKey: r.changeKey,
        seriesKey: r.seriesKey,
        onsetDate: r.onsetDate,
        status: r.status,
        item: ready
          ? {
              text: r.text as string,
              segments: r.segments as Segment[],
              citations: (r.citations ?? []) as Citation[],
            }
          : null,
        aiDisclosure: ready ? ((r.aiDisclosure as AiDisclosure | null) ?? null) : null,
      };
    });
  });
}
