import type { Db } from "@ci/db";
import { weaknessChangeNotes, weaknessSeriesPoint } from "@ci/db/generated";
import { and, eq, sql } from "drizzle-orm";
import { type DayRange, setRange } from "../../../../server/filters/params";
import { readOnly } from "../../../../server/games";
import type { CoachScope } from "../../../../server/profile";
import { ownsScope } from "../../../../server/profile/scope";
import { pointOnSide } from "../../../../server/profile/weaknesses";
import { clipWeeklyRates } from "../_filters/clip";

// Two narrow reads the weaknesses pages need beside T1's read model, each behind its `ownsScope`
// check, read-only and limited to the scope's own account and time class.

/** Weeks of raw weekly rate a list row's sparkline shows (the newest weeks with a point). */
export const ROW_WEEKS = 12;

const TIME_CLASS = {
  bullet: "bullet",
  blitz: "blitz",
  rapid: "rapid",
  classical: "classical",
  daily: "daily",
} as const;
const PLATFORM = { lichess: "lichess", chesscom: "chess.com" } as const;
const count = new Intl.NumberFormat("en-GB");

/** "Based on 214 analysed rapid games on lichess." */
export function basisText(scope: CoachScope): string {
  const o = scope.options.find(
    (x) => x.accountId === scope.accountId && x.timeClass === scope.timeClass,
  );
  const n = Math.max(0, Math.trunc(o?.analysedGames ?? 0));
  const games = `${count.format(n)} analysed ${TIME_CLASS[scope.timeClass]} ${n === 1 ? "game" : "games"}`;
  return `Based on ${games} on ${PLATFORM[scope.platform]}.`;
}

/** The scope's URL parameters, so a link opens the same games coached. */
/**
 * The scope as a query, with the filters that travel between the list and a detail (A4): the
 * colour, so a row opens the same colour's detail, and the dates.
 */
export function scopeQuery(
  scope: CoachScope,
  range?: { from: string | null; to: string | null },
): string {
  const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
  // The page's own range travels, "All time" as `from=all` (ADR 0083 Amendment 2); without one
  // the link opens on the default 90 days.
  if (range) setRange(q, range);
  if (scope.side && scope.side !== "both") q.set("color", scope.side);
  return q.toString();
}

export function weaknessHref(
  seriesKey: string,
  scope: CoachScope,
  range?: { from: string | null; to: string | null },
): string {
  return `/app/weaknesses/${encodeURIComponent(seriesKey)}?${scopeQuery(scope, range)}`;
}

/** Weeks with fewer chances than this are left off a line (hollow on the detail's chart). */
export const WEEKLY_MIN_CHANCES = 20;

const DAY = 86_400_000;
const WEEK = 7 * DAY;

/**
 * Each series' raw weekly rate (events over chances per ISO week, UTC), oldest first, over the
 * scope's newest `ROW_WEEKS` calendar weeks (ending at its newest week with a point). A week with
 * no point, or fewer than `WEEKLY_MIN_CHANCES` chances, is null: a gap, never a zero or a spike.
 * The filter bar's `range` (T3) turns the weeks outside it into gaps too.
 */
export async function weeklyRates(
  db: Db,
  userId: string,
  scope: CoachScope,
  range: DayRange = { from: null, to: null },
): Promise<Map<string, (number | null)[]>> {
  return readOnly(db, async (tx) => {
    const out = new Map<string, (number | null)[]>();
    if (!(await ownsScope(tx, userId, scope))) return out;
    const monday = sql<string>`to_char(date_trunc('week', ${weaknessSeriesPoint.playedAt} at time zone 'UTC'), 'YYYY-MM-DD')`;
    const rows = await tx
      .select({
        seriesKey: weaknessSeriesPoint.seriesKey,
        monday,
        events: sql<number>`sum(${weaknessSeriesPoint.events})::int`,
        chances: sql<number>`sum(${weaknessSeriesPoint.opportunities})::int`,
      })
      .from(weaknessSeriesPoint)
      .where(
        and(
          eq(weaknessSeriesPoint.accountId, scope.accountId),
          eq(weaknessSeriesPoint.timeClass, scope.timeClass),
          // One colour's games when the colour filter names one (ADR 0082).
          pointOnSide(scope),
        ),
      )
      .groupBy(weaknessSeriesPoint.seriesKey, monday);
    const at = (iso: string) => Date.parse(`${iso}T00:00:00Z`);
    const newest = Math.max(...rows.map((r) => at(r.monday)).filter(Number.isFinite));
    if (!Number.isFinite(newest)) return out;
    const first = newest - (ROW_WEEKS - 1) * WEEK;
    for (const r of rows) {
      const i = Math.round((at(r.monday) - first) / WEEK);
      if (!Number.isInteger(i) || i < 0 || i >= ROW_WEEKS) continue;
      const list = out.get(r.seriesKey) ?? Array.from({ length: ROW_WEEKS }, () => null);
      list[i] = r.chances >= WEEKLY_MIN_CHANCES ? r.events / r.chances : null;
      out.set(r.seriesKey, list);
    }
    const firstMonday = new Date(first).toISOString().slice(0, 10);
    for (const [key, list] of out) out.set(key, clipWeeklyRates(list, firstMonday, range));
    return out;
  });
}

/**
 * The change keys of the series' notes written for the scope's time class. A note row carries
 * the time class only in its job input (Rust `WeaknessChangeInput.timeClass`), so a note without
 * one, or for another time class, is never shown on this scope's page.
 */
export async function noteKeysInScope(
  db: Db,
  userId: string,
  scope: CoachScope,
  seriesKey: string,
): Promise<Set<string>> {
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return new Set<string>();
    const rows = await tx
      .select({ changeKey: weaknessChangeNotes.changeKey })
      .from(weaknessChangeNotes)
      .where(
        and(
          eq(weaknessChangeNotes.accountId, scope.accountId),
          eq(weaknessChangeNotes.seriesKey, seriesKey),
          sql`${weaknessChangeNotes.input}->>'timeClass' = ${scope.timeClass}`,
        ),
      );
    return new Set(rows.map((r) => r.changeKey));
  });
}
