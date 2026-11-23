import type { Db } from "@ci/db";
import { focuses, gameMetrics, games, weaknessSeriesPoint } from "@ci/db/generated";
import { and, desc, eq, gt, gte, isNull, lt, sql } from "drizzle-orm";
import { comparisonBacksState } from "../../lib/explain";
import { readOnly, scopedAccountIds } from "../games";
import type { TimeClass } from "../games/types";
import { MODEL_BASELINE_KEYS } from "../profile/benchmark-text";
import { seriesLabel } from "../profile/catalogue";
import type { CoachScope, Side, WeaknessState, WeaknessSummary } from "../profile/types";
import { openingNames, type ScopeKey, summariesIn } from "../profile/weaknesses";
import { type Interval, wilson } from "../stats/wilson";
import {
  chooseTarget,
  DEFAULT_GAMES_N,
  type FocusOutcome,
  focusProgress,
  MAX_GAMES_N,
  MIN_GAMES_N,
  type Progress,
  type Target,
} from "./goal";

export * from "./goal";

// The weekly focus and goal (ADR 0091): one weakness series per user and time class, with a
// target rate per chance over the next N analysed games. Progress is read live from
// `weakness_series_point`; the row is closed once its N games exist. Every read and write is the
// user's own: rows are found by `user_id`, and an account must be one of the user's links.

type Row = typeof focuses.$inferSelect;

export type Focus = {
  id: string;
  accountId: string;
  timeClass: TimeClass;
  seriesKey: string;
  side: Side;
  /** The weakness's plain name (the catalogue's, or the opening's for an opening line). */
  label: string;
  targetRate: number;
  gamesN: number;
  startedAt: Date;
  baseline: { rate: number; events: number; chances: number };
  peerRate: number | null;
  endedAt: Date | null;
  outcome: FocusOutcome | "cleared" | null;
};

export type Suggestion = {
  accountId: string;
  timeClass: TimeClass;
  side: Side;
  seriesKey: string;
  label: string;
  baseline: { rate: number; events: number; chances: number };
  /** The peer median, when the weakness was compared. */
  peer: number | null;
  target: Target;
  gamesN: number;
};

export type FocusCard =
  | { kind: "active"; focus: Focus; progress: Progress }
  | { kind: "closed"; focus: Focus; progress: Progress; next: Suggestion | null }
  | { kind: "suggest"; suggestion: Suggestion }
  /** The user's focus for this time class is on another of their accounts. */
  | { kind: "elsewhere"; focus: Focus }
  | { kind: "none" };

/** States a suggestion may be in: "Costing you games" (ADR 0084). */
const COSTING: ReadonlySet<WeaknessState> = new Set(["active", "emerging", "relapsed"]);

const keyOf = (f: Pick<Row, "accountId" | "timeClass" | "side">): ScopeKey => ({
  accountId: f.accountId,
  timeClass: f.timeClass,
  side: f.side,
});

/** A suggestion for one weakness: its current rate, the peer median and the target. */
export function candidate(
  scope: ScopeKey,
  w: WeaknessSummary,
  gamesN = DEFAULT_GAMES_N,
): Suggestion | null {
  if (w.evidence.chances <= 0) return null;
  // ADR 0101: W5's median is the evaluation's prediction, not a peer rate to aim for.
  const peer = w.compared && !MODEL_BASELINE_KEYS.has(w.seriesKey) ? w.band.median : null;
  const target = chooseTarget({ baseline: w.rate.mean, peer });
  if (!target) return null;
  return {
    accountId: scope.accountId,
    timeClass: scope.timeClass,
    side: scope.side ?? "both",
    seriesKey: w.seriesKey,
    label: w.label,
    baseline: { rate: w.rate.mean, events: w.evidence.events, chances: w.evidence.chances },
    peer,
    target,
    gamesN,
  };
}

/**
 * The default suggestion: the top weakness under "Costing you games" (a current costing state
 * its peer comparison backs, sufficient evidence, ADR 0084), the overview's lead.
 */
async function suggestIn(tx: Db, scope: ScopeKey): Promise<Suggestion | null> {
  const all = await summariesIn(tx, scope);
  for (const w of all) {
    if (
      w.compared &&
      w.priority > 0 &&
      COSTING.has(w.state) &&
      comparisonBacksState(w.state, w.confidence) &&
      w.evidence.sufficient
    ) {
      const c = candidate(scope, w);
      if (c) return c;
    }
  }
  return null;
}

async function labelOf(tx: Db, f: Row): Promise<string> {
  const names = await openingNames(tx, keyOf(f), [f.seriesKey]);
  return seriesLabel(f.seriesKey, names.get(f.seriesKey));
}

async function toFocus(tx: Db, f: Row): Promise<Focus> {
  return {
    id: f.id,
    accountId: f.accountId,
    timeClass: f.timeClass,
    seriesKey: f.seriesKey,
    side: f.side,
    label: await labelOf(tx, f),
    targetRate: f.targetRate,
    gamesN: f.gamesN,
    startedAt: f.startedAt,
    baseline: { rate: f.baselineRate, events: f.baselineK, chances: f.baselineN },
    peerRate: f.peerRate,
    endedAt: f.endedAt,
    outcome: f.outcome,
  };
}

type Counts = { games: number; events: number; chances: number; lastAt: Date | null };

/**
 * The focus's counts: its first `games_n` analysed games after `started_at` (of its colour),
 * optionally only those played in `[from, to)`, and the series' events and chances in them.
 */
export async function focusCounts(
  tx: Db,
  f: Pick<Row, "accountId" | "timeClass" | "side" | "seriesKey" | "startedAt" | "gamesN">,
  within?: { from: Date; to: Date },
): Promise<Counts> {
  const side =
    f.side === "both"
      ? sql`true`
      : sql`${games.playerColor} = ${f.side === "white" ? "white" : "black"}`;
  const inFocus = tx
    .select({ gameId: gameMetrics.gameId, playedAt: gameMetrics.playedAt })
    .from(gameMetrics)
    .innerJoin(games, eq(games.id, gameMetrics.gameId))
    .where(
      and(
        eq(gameMetrics.accountId, f.accountId),
        eq(gameMetrics.timeClass, f.timeClass),
        gt(gameMetrics.playedAt, f.startedAt),
        side,
      ),
    )
    .orderBy(gameMetrics.playedAt, gameMetrics.gameId)
    .limit(f.gamesN)
    .as("in_focus");
  const [row] = await tx
    .select({
      games: sql<number>`count(*)::int`,
      lastAt: sql<string | null>`max(${inFocus.playedAt})`,
      events: sql<number>`coalesce(sum(${weaknessSeriesPoint.events}), 0)::int`,
      chances: sql<number>`coalesce(sum(${weaknessSeriesPoint.opportunities}), 0)::int`,
    })
    .from(inFocus)
    .leftJoin(
      weaknessSeriesPoint,
      and(
        eq(weaknessSeriesPoint.gameId, inFocus.gameId),
        eq(weaknessSeriesPoint.accountId, f.accountId),
        eq(weaknessSeriesPoint.timeClass, f.timeClass),
        eq(weaknessSeriesPoint.seriesKey, f.seriesKey),
      ),
    )
    .where(
      within ? and(gte(inFocus.playedAt, within.from), lt(inFocus.playedAt, within.to)) : undefined,
    );
  return {
    games: row?.games ?? 0,
    events: row?.events ?? 0,
    chances: row?.chances ?? 0,
    lastAt: row?.lastAt ? new Date(row.lastAt) : null,
  };
}

const progressOf = (f: Pick<Row, "gamesN" | "targetRate">, c: Counts) =>
  focusProgress({
    k: c.events,
    n: c.chances,
    games: c.games,
    gamesN: f.gamesN,
    target: f.targetRate,
  });

/** The user's running focus for a time class, locked when `forUpdate`. */
async function activeRow(tx: Db, userId: string, tc: TimeClass, forUpdate = false) {
  const q = tx
    .select()
    .from(focuses)
    .where(and(eq(focuses.userId, userId), eq(focuses.timeClass, tc), isNull(focuses.endedAt)))
    .limit(1);
  const [row] = forUpdate ? await q.for("update") : await q;
  return row ?? null;
}

async function endRow(
  tx: Db,
  f: Row,
  c: Counts,
  outcome: FocusOutcome | "cleared",
  at: Date,
): Promise<void> {
  await tx
    .update(focuses)
    .set({
      endedAt: at,
      outcome,
      resultK: c.events,
      resultN: c.chances,
      resultGames: c.games,
    })
    .where(and(eq(focuses.id, f.id), isNull(focuses.endedAt)));
}

/**
 * Stores the close of a running focus that has reached its N games (R2-7: reads only compute
 * it), on its last game's date with its outcome. Called by the focus POSTs, under the row lock.
 * True when it closed the focus.
 */
async function settle(tx: Db, f: Row): Promise<boolean> {
  const counts = await focusCounts(tx, f);
  const progress = progressOf(f, counts);
  if (!progress.done || !progress.outcome) return false;
  await endRow(tx, f, counts, progress.outcome, counts.lastAt ?? new Date());
  return true;
}

const isGamesN = (n: number) => Number.isInteger(n) && n >= MIN_GAMES_N && n <= MAX_GAMES_N;

export type SetFocusResult =
  | { kind: "set"; focus: Focus }
  | { kind: "not_found" }
  | { kind: "no_target" }
  | { kind: "invalid" };

/**
 * Makes a weakness the user's focus for its time class, starting now: the target comes from
 * its current estimate (never from the client). A running focus for that time class ends as
 * `cleared`, with the counts it reached. `not_found` for an account that is not the user's or a
 * weakness the scope has no estimate for.
 */
export async function setFocus(
  db: Db,
  userId: string,
  req: { accountId: string; timeClass: TimeClass; seriesKey: string; side?: Side; gamesN?: number },
  now: Date = new Date(),
): Promise<SetFocusResult> {
  const gamesN = req.gamesN ?? DEFAULT_GAMES_N;
  if (!isGamesN(gamesN)) return { kind: "invalid" };
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    if ((await scopedAccountIds(tx, userId, req.accountId)).length === 0) {
      return { kind: "not_found" } as const;
    }
    const scope: ScopeKey = {
      accountId: req.accountId,
      timeClass: req.timeClass,
      side: req.side ?? "both",
    };
    const [w] = await summariesIn(tx, scope, req.seriesKey);
    if (!w) return { kind: "not_found" } as const;
    const c = candidate(scope, w, gamesN);
    if (!c) return { kind: "no_target" } as const;
    const running = await activeRow(tx, userId, req.timeClass, true);
    if (running && !(await settle(tx, running))) {
      await endRow(tx, running, await focusCounts(tx, running), "cleared", now);
    }
    const [row] = await tx
      .insert(focuses)
      .values({
        userId,
        accountId: c.accountId,
        timeClass: c.timeClass,
        seriesKey: c.seriesKey,
        side: c.side,
        targetRate: c.target.rate,
        gamesN,
        startedAt: now,
        baselineRate: c.baseline.rate,
        baselineK: c.baseline.events,
        baselineN: c.baseline.chances,
        peerRate: c.peer,
      })
      .returning();
    if (!row) throw new Error("setFocus: insert returned no row");
    return { kind: "set", focus: await toFocus(tx, row) } as const;
  });
}

/**
 * Ends the user's running focus for a time class as `cleared` (or with its outcome, when it has
 * already reached its N games); false when there is none.
 */
export async function clearFocus(
  db: Db,
  userId: string,
  tc: TimeClass,
  now: Date = new Date(),
): Promise<boolean> {
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    const running = await activeRow(tx, userId, tc, true);
    if (!running) return false;
    if (!(await settle(tx, running))) {
      await endRow(tx, running, await focusCounts(tx, running), "cleared", now);
    }
    return true;
  });
}

/**
 * Changes the running focus's N, keeping its start; false when there is none, N is invalid, or
 * the focus has already reached its N games (then its close is stored instead).
 */
export async function setFocusLength(
  db: Db,
  userId: string,
  tc: TimeClass,
  gamesN: number,
): Promise<boolean> {
  if (!isGamesN(gamesN)) return false;
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    const running = await activeRow(tx, userId, tc, true);
    if (!running || (await settle(tx, running))) return false;
    await tx.update(focuses).set({ gamesN }).where(eq(focuses.id, running.id));
    return true;
  });
}

/**
 * The focus card of a scope: the running focus with its live progress; the last one, closed,
 * with the next suggestion; or the suggestion alone. A running focus that has reached its N
 * games reads as closed, on the date of its last game, with its outcome; a read never writes
 * (R2-7), so the next focus POST stores the close (`settle`).
 */
export async function getFocusCard(db: Db, userId: string, scope: CoachScope): Promise<FocusCard> {
  return readOnly(db, async (tx) => {
    const owned = await scopedAccountIds(tx, userId);
    if (!owned.includes(scope.accountId)) return { kind: "none" } as const;
    const key: ScopeKey = { accountId: scope.accountId, timeClass: scope.timeClass, side: "both" };
    const running = await activeRow(tx, userId, scope.timeClass);
    if (running && owned.includes(running.accountId)) {
      if (running.accountId !== scope.accountId) {
        return { kind: "elsewhere", focus: await toFocus(tx, running) } as const;
      }
      const counts = await focusCounts(tx, running);
      const progress = progressOf(running, counts);
      if (!progress.done || !progress.outcome) {
        return { kind: "active", focus: await toFocus(tx, running), progress } as const;
      }
      const focus = await toFocus(tx, {
        ...running,
        endedAt: counts.lastAt,
        outcome: progress.outcome,
      });
      return { kind: "closed", focus, progress, next: await suggestIn(tx, key) } as const;
    }
    const [last] = await tx
      .select()
      .from(focuses)
      .where(
        and(
          eq(focuses.userId, userId),
          eq(focuses.timeClass, scope.timeClass),
          eq(focuses.accountId, scope.accountId),
        ),
      )
      .orderBy(desc(focuses.startedAt), desc(focuses.createdAt))
      .limit(1);
    if (last?.outcome && last.outcome !== "cleared") {
      const progress = progressOf(last, {
        games: last.resultGames ?? 0,
        events: last.resultK ?? 0,
        chances: last.resultN ?? 0,
        lastAt: last.endedAt,
      });
      return {
        kind: "closed",
        focus: await toFocus(tx, last),
        progress: { ...progress, outcome: last.outcome },
        next: await suggestIn(tx, key),
      } as const;
    }
    const suggestion = await suggestIn(tx, key);
    return suggestion ? ({ kind: "suggest", suggestion } as const) : ({ kind: "none" } as const);
  });
}

/** A weakness's focus offer for its detail page: whether it is the focus, else its target. */
export async function focusOffer(
  db: Db,
  userId: string,
  scope: CoachScope,
  w: WeaknessSummary,
): Promise<{ current: boolean; suggestion: Suggestion | null }> {
  const running = await db
    .select()
    .from(focuses)
    .where(
      and(
        eq(focuses.userId, userId),
        eq(focuses.timeClass, scope.timeClass),
        isNull(focuses.endedAt),
      ),
    )
    .limit(1);
  const r = running[0];
  const side = scope.side ?? "both";
  const current =
    r !== undefined &&
    r.accountId === scope.accountId &&
    r.seriesKey === w.seriesKey &&
    r.side === side &&
    // One that reached its N games reads as closed (`getFocusCard`), so it is not the focus.
    !progressOf(r, await focusCounts(db, r)).done;
  return {
    current,
    suggestion: candidate({ accountId: scope.accountId, timeClass: scope.timeClass, side }, w),
  };
}

export type FocusWeekLine = {
  seriesKey: string;
  label: string;
  side: Side;
  targetRate: number;
  events: number;
  chances: number;
  games: number;
  rate: number | null;
  interval: Interval | null;
  /**
   * The days in the week the player practised in the trainer (its `drill_attempts`), the
   * process before the outcome; null where the trainer's tables do not exist.
   */
  practiceSessions: number | null;
};

/** Whether the trainer's tables exist in this database (they come with the trainer lane). */
async function hasTrainer(tx: Db): Promise<boolean> {
  const [row] = await tx.execute<{ ok: boolean }>(
    sql`select to_regclass('public.drill_attempts') is not null and to_regclass('public.drill_items') is not null as ok`,
  );
  return row?.ok === true;
}

/** One reviewed week and the focus that ran in it. */
type WeekFocus = { week: string; from: Date; to: Date; focus: Row };

/** The weeks as a `values` list: (week, focus id, from, to). */
const weekValues = (weeks: readonly WeekFocus[]) =>
  sql.join(
    weeks.map(
      (w) =>
        sql`(${w.week}::text, ${w.focus.id}::uuid, ${w.from.toISOString()}::timestamptz, ${w.to.toISOString()}::timestamptz)`,
    ),
    sql`, `,
  );

/**
 * Each week's focus counts in one query (`focusCounts` with `within`, for every week at once):
 * the focus's first `games_n` analysed games after its start (of its colour) played in the week,
 * and the series' events and chances in them.
 */
async function weekCounts(tx: Db, weeks: readonly WeekFocus[]): Promise<Map<string, Counts>> {
  const rows = (await tx.execute(sql`
    with wk(week, focus_id, from_at, to_at) as (values ${weekValues(weeks)}),
    f as (select * from focuses where id in (select focus_id from wk)),
    in_focus as (
      select f.id as focus_id, f.account_id, f.time_class, f.series_key, g.game_id, g.played_at
      from f
      cross join lateral (
        select gm.game_id, gm.played_at
        from game_metrics gm
        join games on games.id = gm.game_id
        where gm.account_id = f.account_id and gm.time_class = f.time_class
          and gm.played_at > f.started_at
          and (f.side = 'both' or games.player_color::text = f.side::text)
        order by gm.played_at, gm.game_id
        limit f.games_n
      ) g
    )
    select wk.week, count(*)::int as games, max(i.played_at) as last_at,
      coalesce(sum(p.events), 0)::int as events,
      coalesce(sum(p.opportunities), 0)::int as chances
    from wk
    join in_focus i on i.focus_id = wk.focus_id
      and i.played_at >= wk.from_at and i.played_at < wk.to_at
    left join weakness_series_point p
      on p.game_id = i.game_id and p.account_id = i.account_id
      and p.time_class = i.time_class and p.series_key = i.series_key
    group by wk.week`)) as unknown as Record<string, unknown>[];
  return new Map(
    rows.map((r) => [
      String(r.week),
      {
        games: Number(r.games),
        events: Number(r.events),
        chances: Number(r.chances),
        lastAt: r.last_at == null ? null : new Date(String(r.last_at)),
      },
    ]),
  );
}

/**
 * Each week's practice in one query: the distinct UTC days in the week with a trainer attempt on
 * the target's positions.
 */
async function weekPracticeDays(
  tx: Db,
  userId: string,
  target: { accountId: string; timeClass: TimeClass },
  weeks: readonly WeekFocus[],
): Promise<Map<string, number>> {
  const rows = (await tx.execute(sql`
    with wk(week, focus_id, from_at, to_at) as (values ${weekValues(weeks)})
    select wk.week, count(distinct (a.at at time zone 'UTC')::date)::int as n
    from wk
    join drill_attempts a on a.user_id = ${userId} and a.at >= wk.from_at and a.at < wk.to_at
    join drill_items i on i.id = a.item_id
    where i.user_id = ${userId} and i.account_id = ${target.accountId}
      and i.time_class = ${target.timeClass}
    group by wk.week`)) as unknown as { week: string; n: number }[];
  return new Map(rows.map((r) => [String(r.week), Number(r.n)]));
}

/**
 * "Your focus this week" for each weekly review (ADR 0091): the focus that ran during the week
 * starting on each Monday (`YYYY-MM-DD`), with its own games' counts in that week. A week before
 * any focus, or with none of a focus's games, gets no line. Only the user's own focuses of the
 * target's account and time class are read.
 */
export async function focusWeekLines(
  db: Db,
  userId: string,
  target: { accountId: string; timeClass: TimeClass },
  weekStarts: string[],
): Promise<Map<string, FocusWeekLine>> {
  if (weekStarts.length === 0) return new Map();
  return readOnly(db, async (tx) => {
    if ((await scopedAccountIds(tx, userId, target.accountId)).length === 0) return new Map();
    return focusWeekLinesIn(tx, userId, target, weekStarts);
  });
}

/**
 * `focusWeekLines` inside a caller's transaction that has already checked the account: a fixed
 * number of queries for any number of weeks (the focuses, the counts, the labels, the practice).
 */
export async function focusWeekLinesIn(
  tx: Db,
  userId: string,
  target: { accountId: string; timeClass: TimeClass },
  weekStarts: string[],
): Promise<Map<string, FocusWeekLine>> {
  const out = new Map<string, FocusWeekLine>();
  if (weekStarts.length === 0) return out;
  const rows = await tx
    .select()
    .from(focuses)
    .where(
      and(
        eq(focuses.userId, userId),
        eq(focuses.accountId, target.accountId),
        eq(focuses.timeClass, target.timeClass),
      ),
    )
    .orderBy(desc(focuses.startedAt));
  if (rows.length === 0) return out;
  const weeks = weekStarts.flatMap((week): WeekFocus[] => {
    const from = new Date(`${week}T00:00:00Z`);
    if (Number.isNaN(from.getTime())) return [];
    const to = new Date(from.getTime() + 7 * 86_400_000);
    // The newest focus that started before the week ended and had not ended before it began.
    const focus = rows.find((r) => r.startedAt < to && (r.endedAt === null || r.endedAt >= from));
    return focus ? [{ week, from, to, focus }] : [];
  });
  if (weeks.length === 0) return out;
  const counts = await weekCounts(tx, weeks);
  const shown = weeks.filter((w) => (counts.get(w.week)?.games ?? 0) > 0);
  if (shown.length === 0) return out;
  const scope: ScopeKey = { ...target, side: "both" };
  const names = await openingNames(tx, scope, [...new Set(shown.map((w) => w.focus.seriesKey))]);
  const practice = (await hasTrainer(tx))
    ? await weekPracticeDays(tx, userId, target, shown)
    : null;
  for (const { week, focus: f } of shown) {
    const c = counts.get(week);
    if (!c) continue;
    out.set(week, {
      seriesKey: f.seriesKey,
      label: seriesLabel(f.seriesKey, names.get(f.seriesKey)),
      side: f.side,
      targetRate: f.targetRate,
      events: c.events,
      chances: c.chances,
      games: c.games,
      rate: c.chances > 0 ? c.events / c.chances : null,
      interval: wilson(c.events, c.chances, 0.8),
      practiceSessions: practice ? (practice.get(week) ?? 0) : null,
    });
  }
  return out;
}
