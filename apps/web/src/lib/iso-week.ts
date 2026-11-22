// ISO week keys ("2026-W46"), as `kpi_snapshot.window_key` stores the weekly rows, turned into
// the day the charts plot. Shared by the progress page and the skill-area pages (B2: the area
// pages once passed the raw key to the chart, which read it as a date and found none).

const DAY = 86_400_000;

/** "2026-W46" to that ISO week's Monday, "2026-11-09"; null when it is not a real ISO week. */
export function weekMonday(week: string): string | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(week);
  if (!m) return null;
  const year = Number(m[1]);
  const w = Number(m[2]);
  if (w < 1 || w > 53) return null;
  const jan4 = Date.UTC(year, 0, 4);
  const dow = (new Date(jan4).getUTCDay() + 6) % 7;
  const monday = jan4 - dow * DAY + (w - 1) * 7 * DAY;
  // The week's Thursday decides its ISO year (week 53 exists only in some years).
  if (new Date(monday + 3 * DAY).getUTCFullYear() !== year) return null;
  return new Date(monday).toISOString().slice(0, 10);
}

/** Trend points keyed by ISO week, re-keyed by their Monday; rows that aren't weeks are dropped. */
export function weeklyPoints<P extends { week: string }>(points: readonly P[]): P[] {
  return points.flatMap((p) => {
    const week = weekMonday(p.week);
    return week ? [{ ...p, week }] : [];
  });
}
