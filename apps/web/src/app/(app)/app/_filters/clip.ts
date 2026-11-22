import { weekMonday } from "../../../../lib/iso-week";
import type { DayRange } from "../../../../server/filters/params";

// The date filter on stored weekly series (T3): the series are weekly snapshots, so a range clips
// whole weeks. A week stays when any of its seven days is inside the range; a dense series keeps
// its length and turns clipped weeks into gaps (null), never zeros.

const DAY = 86_400_000;
const plusDays = (iso: string, n: number) =>
  new Date(Date.parse(`${iso}T00:00:00Z`) + n * DAY).toISOString().slice(0, 10);

/** `week` is an ISO week ("2026-W40") or its Monday ("2026-09-28"). */
export function weekOverlaps(week: string, range: DayRange): boolean {
  const monday = /^\d{4}-W\d{2}$/.test(week) ? weekMonday(week) : week;
  if (!monday) return false;
  const sunday = plusDays(monday, 6);
  return (range.from === null || sunday >= range.from) && (range.to === null || monday <= range.to);
}

const unbounded = (r: DayRange) => r.from === null && r.to === null;

export function clipWeekly<T extends { week: string }>(points: T[], range: DayRange): T[] {
  if (unbounded(range)) return points;
  return points.filter((p) => weekOverlaps(p.week, range));
}

/** A dense weekly series starting on `firstMonday`, with weeks outside `range` as gaps. */
export function clipWeeklyRates(
  values: (number | null)[],
  firstMonday: string,
  range: DayRange,
): (number | null)[] {
  if (unbounded(range)) return values;
  return values.map((v, i) => (weekOverlaps(plusDays(firstMonday, 7 * i), range) ? v : null));
}
