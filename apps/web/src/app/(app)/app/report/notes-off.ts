import type { CoachStatus } from "../../../../server/coach/status";
import { COACH_UNAVAILABLE_TITLE } from "../../../../server/coach/status-copy";
import { whenText } from "./rebuild-button";

/**
 * Why a ready report has no coach's notes (Fix D, N2), for the report banner and the area pages'
 * "Coach's note". A report built from the numbers alone says two different things:
 * - the coach cannot write now (`ai_status`, ADR 0078): "Coach not available right now";
 * - the coach is on, but this report was built before it was (its run stored `no_api_key`):
 *   the notes come with the next rebuild, from `nextManualAt` (null is "now").
 * The budget has its own copy at each caller, so it never reaches here.
 */
export function numbersOnlyCopy({
  coach,
  nextManualAt,
  now,
}: {
  coach: CoachStatus;
  nextManualAt: Date | null;
  now: Date;
}): { kind: "unavailable" | "computed_only"; title: string; body: string } {
  if (coach !== "available") {
    return {
      kind: "unavailable",
      title: COACH_UNAVAILABLE_TITLE,
      body: "This report shows the numbers computed from your games, without your coach's notes.",
    };
  }
  const when = nextManualAt && nextManualAt > now ? whenText(nextManualAt, now) : "now";
  return {
    kind: "computed_only",
    title: "This report was built from your numbers only",
    body: `Your coach's notes come with the next rebuild, available ${when}.`,
  };
}

/** The line a section shows in place of its notes when the report is numbers-only. */
export const NUMBERS_ONLY_SECTION =
  "This report was built from your numbers only, so this section shows the computed numbers.";
