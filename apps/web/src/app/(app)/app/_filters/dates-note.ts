import type { DayRange } from "../../../../server/filters/params";
import { ESTIMATE_WINDOW_TEXT } from "../../../../server/profile/window";
import { rangeWords } from "./model";

// The honest line under the filter bar of a page whose dates only clip its weekly lines (audit
// A6): the stored estimates and last-50 values have no dates, so the page says what the dates
// change and what stays on its own window, as Progress does for its trends.

const KEEPS = {
  weaknesses: `Each rate, its range, its state and the comparison stay on your games ${ESTIMATE_WINDOW_TEXT}.`,
  area: `The score and each measure's current value stay on your last 50 games, and the weakness rates on your games ${ESTIMATE_WINDOW_TEXT}.`,
} as const;

/** "1 Sep to 31 Oct 2026: these dates trim the weekly lines only. …"; null without dates. */
export function datesNote(range: DayRange, page: keyof typeof KEEPS): string | null {
  if (range.from === null && range.to === null) return null;
  const words = rangeWords(range);
  return `${words.charAt(0).toUpperCase()}${words.slice(1)}: these dates trim the weekly lines only. ${KEEPS[page]}`;
}

const REPORT_WINDOW = `The report is written from your whole profile, on all your games of this time control: the weakness rates cover the ${ESTIMATE_WINDOW_TEXT.replace(/^in the /, "")}.`;

/**
 * The report's window, always stated (ADR 0083 Amendment 2): the coach's text and the sections
 * come from the profile, which no date filter narrows; the dates trim the weekly lines only.
 */
export function reportWindowNote(range: DayRange): string {
  if (range.from === null && range.to === null) return REPORT_WINDOW;
  const words = rangeWords(range);
  return `${words.charAt(0).toUpperCase()}${words.slice(1)}: these dates trim the weekly lines only. ${REPORT_WINDOW}`;
}
