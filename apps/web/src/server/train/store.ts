import { fenKey } from "@ci/chess-core";
import type { Db } from "@ci/db";
import { drillAttempts, drillItems, games } from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, eq, isNull, type SQL, sql } from "drizzle-orm";
import { registerAlgorithm } from "../algorithm";
import { readOnly, scopedAccountIds } from "../games";
import type { CoachScope } from "../profile/types";
import {
  acceptedSet,
  addDays,
  afterAttempt,
  buildSession,
  type Candidate,
  type Confidence,
  dayOf,
  feedback,
  grade,
  isHeldOut,
  isSourceKey,
  keepCandidate,
  type MultipvLine,
  type PractiseToday,
  practiseToday,
  primarySource,
  type QueueItem,
  type Rate,
  rateOf,
  SOURCES,
  type SourceKey,
  sanOf,
  startOfDay,
  type Verdict,
  verdictOf,
  WEAKNESS_SOURCES,
} from "./model";

// The trainer's rows (ADR 0090): items are made from the scope's stored analysis by a POST, at
// most once per set of analysed games (new positions only, never rewritten; Amendment 1), served
// by `buildSession`, and graded against the stored accepted set. Reads never write. Every read and write is the user's own: items carry `user_id`, and the scope's
// account must be one of the user's links.

const fin = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const num = (v: unknown): number | null => (v == null ? null : Number(v));
const UCI = /^[a-h][1-8][a-h][1-8][qrbn]?$/;

function scoreOf(v: unknown): MultipvLine["score"] | null {
  if (typeof v !== "object" || v === null) return null;
  const o = v as Record<string, unknown>;
  if (fin(o.mate) && o.cp === undefined) return { mate: o.mate };
  if (fin(o.cp) && o.mate === undefined) return { cp: o.cp };
  return null;
}

/** Stored MultiPV lines (`position_multipv.lines`), each checked; a bad line is dropped. */
export function parseLines(v: unknown): MultipvLine[] {
  if (!Array.isArray(v)) return [];
  return v.flatMap((l): MultipvLine[] => {
    if (typeof l !== "object" || l === null) return [];
    const o = l as Record<string, unknown>;
    const score = scoreOf(o.score);
    if (typeof o.uci !== "string" || !UCI.test(o.uci) || !score) return [];
    const pv = Array.isArray(o.pv) ? o.pv.filter((m): m is string => typeof m === "string") : [];
    return [{ uci: o.uci, score, pv }];
  });
}

const tagsOf = (bestLine: unknown): string[] => {
  if (typeof bestLine !== "object" || bestLine === null) return [];
  const t = (bestLine as Record<string, unknown>).tags;
  return Array.isArray(t) ? t.filter((x): x is string => typeof x === "string").slice(0, 8) : [];
};

/** Whether the scope's account is one of the user's links. */
async function owns(tx: Db, userId: string, accountId: string): Promise<boolean> {
  return (await scopedAccountIds(tx, userId, accountId)).length > 0;
}

type Scope = Pick<CoachScope, "accountId" | "timeClass">;

/** A Postgres array literal of bound values (an empty one included). */
function arrayOf(xs: readonly (string | number)[], type: "text" | "uuid" | "int") {
  const cast = sql.raw(`${type}[]`);
  if (xs.length === 0) return sql`array[]::${cast}`;
  return sql`array[${sql.join(
    xs.map((x) => sql`${x}`),
    sql`, `,
  )}]::${cast}`;
}

// ---- Freshness: items are made by a POST, at most once per set of analysed games ----

/**
 * The scope's source summary: its games (how many, how many analysed, their analysis
 * generations, the newest analysis) and its profile rows (how many, the newest write). Items
 * come only from these rows, so the same summary means no new position. Two aggregates over the
 * indexed rows of one account and time class.
 */
async function sourceMark(tx: Db, scope: Scope): Promise<string> {
  const [row] = (await tx.execute(sql`
    select concat_ws(':', g.n, g.done, g.generations, g.analysed, m.n, m.written) as mark
    from (
      select count(*) as n,
        count(*) filter (where analysis_status = 'done') as done,
        coalesce(sum(analysed_generation), 0) as generations,
        coalesce(extract(epoch from max(analysed_at))::text, '') as analysed
      from games
      where account_id = ${scope.accountId} and time_class = ${scope.timeClass}::time_class
    ) g, (
      select count(*) as n, coalesce(extract(epoch from max(created_at))::text, '') as written
      from game_metrics
      where account_id = ${scope.accountId} and time_class = ${scope.timeClass}::time_class
    ) m`)) as unknown as { mark: string }[];
  return String(row?.mark ?? "");
}

/** The summary the user's items were last made from, or null before the first refresh. */
async function storedMark(tx: Db, userId: string, scope: Scope): Promise<string | null> {
  const [row] = (await tx.execute(sql`
    select source_mark from drill_refreshes
    where user_id = ${userId} and account_id = ${scope.accountId}
      and time_class = ${scope.timeClass}::time_class`)) as unknown as { source_mark: string }[];
  return row ? String(row.source_mark) : null;
}

/** Whether new analysed games or profile rows landed since the items were last made (a read). */
async function isStale(tx: Db, userId: string, scope: Scope): Promise<boolean> {
  return (await storedMark(tx, userId, scope)) !== (await sourceMark(tx, scope));
}

/** Candidate rows read per page. */
export const MAX_CANDIDATES = 400;
/** Pages read per refresh at most (the newest positions first). */
const MAX_PAGES = 25;

type Kept = { c: Candidate; r: Record<string, unknown>; playedAt: Date };

/**
 * The scope's drillable positions the user has no item for yet: the player's own moves at a
 * weakness exemplar ply (`W4.missed`, `W3.*`, `W7.traps`) or judged a blunder, kept by
 * `keepCandidate`, newest game first. A read: `refreshItems` stores them, and the overview and
 * the weakness page count them before it has.
 */
async function candidates(tx: Db, userId: string, scope: Scope): Promise<Kept[]> {
  const keys = sql.join(
    WEAKNESS_SOURCES.map((k) => sql`${k}::text`),
    sql`, `,
  );
  const out: Kept[] = [];
  let after: { at: string; game: string; ply: number } | null = null;
  for (let page = 0; page < MAX_PAGES; page++) {
    const cursor: SQL = after
      ? sql`and (g.played_at, g.id, m.ply) < (${after.at}::timestamptz, ${after.game}::uuid, ${after.ply}::int)`
      : sql``;
    const rows = (await tx.execute(sql`
      select c.*, pm.san as prev_san, f.best_pv, f.best_line, f.multipv
      from (
        select g.id as game_id, g.played_at, g.played_at::text as played_key, m.ply, m.fen_before,
          m.uci, m.best_uci, m.win_before, m.win_after, m.judgement::text as judgement,
          array(
            select k from unnest(array[${keys}]) as k
            where jsonb_typeof(gm.metrics -> 'weakness' -> k -> 'exemplarPlies') = 'array'
              and (gm.metrics -> 'weakness' -> k -> 'exemplarPlies') @> to_jsonb(m.ply)
          ) as w_sources
        from games g
        join moves m on m.game_id = g.id and m.color = g.player_color
        left join game_metrics gm on gm.game_id = g.id
        where g.account_id = ${scope.accountId} and g.time_class = ${scope.timeClass}::time_class
          and m.best_uci is not null and m.best_uci <> m.uci
          ${cursor}
          and not exists (
            select 1 from drill_items d
            where d.user_id = ${userId} and d.game_id = g.id and d.ply = m.ply)
      ) c
      left join moves pm on pm.game_id = c.game_id and pm.ply = c.ply - 1
      left join move_features f on f.game_id = c.game_id and f.ply = c.ply
      where c.judgement = 'blunder' or cardinality(c.w_sources) > 0
      order by c.played_at desc, c.game_id desc, c.ply desc
      limit ${MAX_CANDIDATES}`)) as unknown as Record<string, unknown>[];
    for (const r of rows) {
      const winBefore = num(r.win_before);
      const winAfter = num(r.win_after);
      if (!fin(winBefore) || !fin(winAfter)) continue;
      const sources = [
        ...(Array.isArray(r.w_sources) ? r.w_sources : []).filter(isSourceKey),
        ...(r.judgement === "blunder" ? (["blunder"] as const) : []),
      ];
      const c: Candidate = {
        gameId: String(r.game_id),
        ply: Number(r.ply),
        fen: String(r.fen_before),
        uci: String(r.uci),
        bestUci: r.best_uci == null ? null : String(r.best_uci),
        winBefore,
        winAfter,
        prevSan: r.prev_san == null ? null : String(r.prev_san),
        sources: SOURCES.filter((s) => sources.includes(s)),
      };
      if (!keepCandidate(c)) continue;
      const playedAt = r.played_at instanceof Date ? r.played_at : new Date(String(r.played_at));
      out.push({ c, r, playedAt });
    }
    const last = rows.at(-1);
    if (rows.length < MAX_CANDIDATES || !last) break;
    after = { at: String(last.played_key), game: String(last.game_id), ply: Number(last.ply) };
  }
  return out;
}

/** A position not yet stored, as the session would see it on the day it is made. */
const pendingItem = (k: Kept): QueueItem => ({
  id: `new:${k.c.gameId}:${k.c.ply}`,
  sourceKey: primarySource(k.c.sources),
  sourceKeys: k.c.sources,
  dueOn: null,
  introducedOn: null,
  heldOut: isHeldOut(k.c.gameId, k.c.ply),
  retired: false,
  playedAt: k.playedAt,
});

/** The positions a refresh would add, when new games landed since the last one (a read). */
async function pendingItems(tx: Db, userId: string, scope: Scope): Promise<QueueItem[]> {
  if (!(await isStale(tx, userId, scope))) return [];
  return (await candidates(tx, userId, scope)).map(pendingItem);
}

/** The rows of kept positions: each with its accepted set from the stored alternatives. */
async function rowsOf(tx: Db, userId: string, scope: Scope, kept: readonly Kept[]) {
  // The alternatives: the stored MultiPV of each position (any protocol, the newest), else
  // the move's own copy in `move_features.multipv`.
  const fenKeys = [...new Set(kept.map((k) => fenKey(k.c.fen)))];
  const mpv = (await tx.execute(sql`
    select distinct on (fen_key) fen_key, lines from position_multipv
    where fen_key = any(${arrayOf(fenKeys, "text")})
    order by fen_key, created_at desc`)) as unknown as { fen_key: string; lines: unknown }[];
  const linesBy = new Map(mpv.map((m) => [m.fen_key, parseLines(m.lines)]));
  return kept.map(({ c, r }) => {
    const best = c.bestUci as string;
    const stored = linesBy.get(fenKey(c.fen));
    const lines = stored && stored.length > 0 ? stored : parseLines(r.multipv);
    const { accepted, mateIn } = acceptedSet(best, lines);
    const pv = Array.isArray(r.best_pv)
      ? r.best_pv.filter((m): m is string => typeof m === "string" && UCI.test(m))
      : [];
    return {
      userId,
      accountId: scope.accountId,
      timeClass: scope.timeClass,
      gameId: c.gameId,
      ply: c.ply,
      sourceKey: primarySource(c.sources),
      sourceKeys: c.sources,
      fen: c.fen,
      playedUci: c.uci,
      solutionUci: best,
      solutionLine: pv[0] === best ? pv.slice(0, 16) : [best],
      acceptedUci: accepted,
      mateIn,
      winBefore: c.winBefore,
      winAfter: c.winAfter,
      tags: tagsOf(r.best_line),
      heldOut: isHeldOut(c.gameId, c.ply),
    };
  });
}

/**
 * Adds the scope's new drillable positions for the user (`candidates`), when its analysed games
 * or profile rows changed since the last refresh; otherwise it reads two aggregates and returns.
 * Existing items are never touched. Called by `POST /api/train/refresh` (the trainer and the
 * overview ask for it when a read says new positions wait), never on a GET. Returns how many were
 * added.
 */
export async function refreshItems(db: Db, userId: string, scope: Scope): Promise<number> {
  return db.transaction(async (t) => {
    const tx = t as unknown as Db;
    if (!(await owns(tx, userId, scope.accountId))) return 0;
    // One refresh of a user's scope at a time: a second waits, then finds the mark current.
    const lock = `drill-refresh:${userId}:${scope.accountId}:${scope.timeClass}`;
    await tx.execute(sql`select pg_advisory_xact_lock(hashtextextended(${lock}, 0))`);
    const mark = await sourceMark(tx, scope);
    if ((await storedMark(tx, userId, scope)) === mark) return 0;
    const kept = await candidates(tx, userId, scope);
    const algorithmId = kept.length > 0 ? await registerAlgorithm(tx) : null;
    let added = 0;
    for (let i = 0; i < kept.length; i += MAX_CANDIDATES) {
      const values = await rowsOf(tx, userId, scope, kept.slice(i, i + MAX_CANDIDATES));
      const rows = await tx
        .insert(drillItems)
        .values(values.map((v) => ({ ...v, algorithmId })))
        .onConflictDoNothing()
        .returning({ id: drillItems.id });
      added += rows.length;
    }
    await tx.execute(sql`
      insert into drill_refreshes (user_id, account_id, time_class, source_mark)
      values (${userId}, ${scope.accountId}, ${scope.timeClass}::time_class, ${mark})
      on conflict (user_id, account_id, time_class)
      do update set source_mark = excluded.source_mark, refreshed_at = now()`);
    return added;
  });
}

// ---- Reads (never a write: R2-7) ----

export type TrainItem = {
  id: string;
  fen: string;
  orientation: "white" | "black";
  /** The opponent's move that led to the position, for the board's last-move tint. */
  lastMove: { from: string; to: string } | null;
  /** "Move 21, White to move". */
  label: string;
};

export type EfficacyRow = {
  source: SourceKey;
  /** Items of the source the player has drilled at least once. */
  drilled: number;
  firstDrill: Date | null;
  before: Rate;
  after: Rate;
  verdict: Verdict;
};

export type PatternCompare = {
  since: Date | null;
  drilled: Rate;
  heldOut: Rate;
  verdict: Verdict;
};

export type TrainPage = {
  source: SourceKey | null;
  /**
   * New analysed games landed since the items were last made: the page asks for them
   * (`POST /api/train/refresh`) and reloads.
   */
  stale: boolean;
  session: TrainItem[];
  doneToday: number;
  rightToday: number;
  dueTomorrow: number;
  counts: { active: number; fresh: number; heldOut: number; retired: number };
  efficacy: { rows: EfficacyRow[]; patterns: PatternCompare };
};

/** When the day is counted: `now`, in the player's time zone (UTC when none is known). */
type Clock = { now: Date; timeZone?: string };

function moveLabel(fen: string, ply: number): string {
  const white = (fen.split(" ")[1] ?? "w") === "w";
  return `Move ${Math.ceil(ply / 2)}, ${white ? "White" : "Black"} to move`;
}

const dateOf = (v: unknown): Date => (v instanceof Date ? v : new Date(String(v)));

/** The user's stored items of the scope, with their game's date. */
function storedItems(tx: Db, userId: string, scope: Scope) {
  return tx
    .select({
      id: drillItems.id,
      gameId: drillItems.gameId,
      ply: drillItems.ply,
      fen: drillItems.fen,
      sourceKey: drillItems.sourceKey,
      sourceKeys: drillItems.sourceKeys,
      dueOn: drillItems.dueOn,
      introducedOn: drillItems.introducedOn,
      heldOut: drillItems.heldOut,
      retiredAt: drillItems.retiredAt,
      tags: drillItems.tags,
      playedAt: games.playedAt,
    })
    .from(drillItems)
    .innerJoin(games, eq(games.id, drillItems.gameId))
    .where(
      and(
        eq(drillItems.userId, userId),
        eq(drillItems.accountId, scope.accountId),
        eq(drillItems.timeClass, scope.timeClass),
      ),
    );
}

type Stored = Awaited<ReturnType<typeof storedItems>>[number];

const queueItem = (i: Stored): QueueItem => ({
  id: i.id,
  sourceKey: isSourceKey(i.sourceKey) ? i.sourceKey : "blunder",
  sourceKeys: i.sourceKeys.filter(isSourceKey),
  dueOn: i.dueOn,
  introducedOn: i.introducedOn,
  heldOut: i.heldOut,
  retired: i.retiredAt !== null,
  playedAt: i.playedAt,
});

/** The scope's attempts, one row per attempted item: its first ever, and today's. */
async function attemptsByItem(tx: Db, userId: string, scope: Scope, todayStart: Date) {
  const rows = (await tx.execute(sql`
    select a.item_id, min(a.at) as first_at,
      count(*) filter (where a.at >= ${todayStart.toISOString()}::timestamptz)::int as today_n,
      count(*) filter (where a.at >= ${todayStart.toISOString()}::timestamptz and a.correct)::int
        as today_right
    from drill_attempts a
    join drill_items i on i.id = a.item_id
    where a.user_id = ${userId} and i.user_id = ${userId}
      and i.account_id = ${scope.accountId} and i.time_class = ${scope.timeClass}::time_class
    group by a.item_id`)) as unknown as Record<string, unknown>[];
  return rows.map((r) => ({
    itemId: String(r.item_id),
    firstAt: dateOf(r.first_at),
    todayN: Number(r.today_n),
    todayRight: Number(r.today_right),
  }));
}

/** How many of the scope's items were attempted since `todayStart`. */
async function attemptedSince(
  tx: Db,
  userId: string,
  scope: Scope,
  todayStart: Date,
): Promise<number> {
  const [row] = (await tx.execute(sql`
    select count(distinct a.item_id)::int as n
    from drill_attempts a
    join drill_items i on i.id = a.item_id
    where a.user_id = ${userId} and i.user_id = ${userId}
      and i.account_id = ${scope.accountId} and i.time_class = ${scope.timeClass}::time_class
      and a.at >= ${todayStart.toISOString()}::timestamptz`)) as unknown as { n: number }[];
  return Number(row?.n ?? 0);
}

/** The opponent's move before each item's position: the exact (game, ply - 1) pairs. */
async function previousMoves(
  tx: Db,
  at: readonly { gameId: string; ply: number }[],
): Promise<Map<string, string>> {
  if (at.length === 0) return new Map();
  const rows = (await tx.execute(sql`
    select m.game_id, m.ply, m.uci
    from unnest(${arrayOf(
      at.map((p) => p.gameId),
      "uuid",
    )}, ${arrayOf(
      at.map((p) => p.ply - 1),
      "int",
    )}) as p(game_id, ply)
    join moves m on m.game_id = p.game_id and m.ply = p.ply`)) as unknown as Record<
    string,
    unknown
  >[];
  return new Map(rows.map((r) => [`${String(r.game_id)}#${Number(r.ply)}`, String(r.uci)]));
}

type Split = { before: { events: number; chances: number }; after: typeof zero };
const zero = { events: 0, chances: 0 };

/**
 * Each weakness source's events and chances in the scope's games before and from its first
 * drill (`firsts`, null: all before), summed in SQL from `game_metrics.metrics.weakness`. A game
 * counts for a source when its events and opportunities are numbers and it had a chance.
 */
async function weaknessSplits(
  tx: Db,
  scope: Scope,
  firsts: readonly { source: SourceKey; first: Date | null }[],
): Promise<Map<SourceKey, Split>> {
  if (firsts.length === 0) return new Map();
  const values = sql.join(
    firsts.map(
      (f) => sql`(${f.source}::text, ${f.first ? f.first.toISOString() : null}::timestamptz)`,
    ),
    sql`, `,
  );
  const rows = (await tx.execute(sql`
    select s.source,
      coalesce(sum(x.events) filter (where not x.after), 0)::float8 as be,
      coalesce(sum(x.chances) filter (where not x.after), 0)::float8 as bn,
      coalesce(sum(x.events) filter (where x.after), 0)::float8 as ae,
      coalesce(sum(x.chances) filter (where x.after), 0)::float8 as an
    from (values ${values}) as s(source, first_at)
    join game_metrics gm
      on gm.account_id = ${scope.accountId} and gm.time_class = ${scope.timeClass}::time_class
    cross join lateral (select gm.metrics -> 'weakness' -> s.source as w) wv
    cross join lateral (
      select
        case when jsonb_typeof(wv.w -> 'events') = 'number'
          then (wv.w ->> 'events')::float8 end as events,
        case when jsonb_typeof(wv.w -> 'opportunities') = 'number'
          then (wv.w ->> 'opportunities')::float8 end as chances,
        gm.played_at >= coalesce(s.first_at, 'infinity'::timestamptz) as after
    ) x
    where jsonb_typeof(wv.w) = 'object'
      and x.events is not null and x.chances is not null and x.chances > 0
    group by s.source`)) as unknown as Record<string, unknown>[];
  return new Map(
    rows.flatMap((r): [SourceKey, Split][] =>
      isSourceKey(r.source)
        ? [
            [
              r.source,
              {
                before: { events: Number(r.be), chances: Number(r.bn) },
                after: { events: Number(r.ae), chances: Number(r.an) },
              },
            ],
          ]
        : [],
    ),
  );
}

/** The player's blunders per own move, before and from `first`, counted in SQL. */
async function blunderSplit(tx: Db, scope: Scope, first: Date | null): Promise<Split> {
  const at = sql`coalesce(${first ? first.toISOString() : null}::timestamptz, 'infinity'::timestamptz)`;
  const [r] = (await tx.execute(sql`
    select
      count(*) filter (where m.judgement = 'blunder' and g.played_at < ${at})::int as be,
      count(*) filter (where g.played_at < ${at})::int as bn,
      count(*) filter (where m.judgement = 'blunder' and g.played_at >= ${at})::int as ae,
      count(*) filter (where g.played_at >= ${at})::int as an
    from games g join moves m on m.game_id = g.id and m.color = g.player_color
    where g.account_id = ${scope.accountId}
      and g.time_class = ${scope.timeClass}::time_class`)) as unknown as Record<string, unknown>[];
  return {
    before: { events: Number(r?.be ?? 0), chances: Number(r?.bn ?? 0) },
    after: { events: Number(r?.ae ?? 0), chances: Number(r?.an ?? 0) },
  };
}

/**
 * `patternFound` in SQL for two tag sets: the chances at the motifs (`missedByMotif` of each game
 * played since `since`) and how many of them the player did not miss.
 */
async function patternRates(
  tx: Db,
  scope: Scope,
  since: Date,
  drilled: readonly string[],
  held: readonly string[],
): Promise<{ drilled: Rate; held: Rate }> {
  if (drilled.length === 0 && held.length === 0) {
    return { drilled: rateOf(0, 0), held: rateOf(0, 0) };
  }
  const d = arrayOf(drilled, "text");
  const h = arrayOf(held, "text");
  const [r] = (await tx.execute(sql`
    select
      coalesce(sum(y.found) filter (where e.key = any(${d})), 0)::float8 as df,
      coalesce(sum(y.n) filter (where e.key = any(${d})), 0)::float8 as dn,
      coalesce(sum(y.found) filter (where e.key = any(${h})), 0)::float8 as hf,
      coalesce(sum(y.n) filter (where e.key = any(${h})), 0)::float8 as hn
    from game_metrics gm
    cross join lateral jsonb_each(
      case when jsonb_typeof(gm.metrics -> 'missedByMotif') = 'object'
        then gm.metrics -> 'missedByMotif' else '{}'::jsonb end) e
    cross join lateral (
      select
        case when jsonb_typeof(e.value -> 'opportunities') = 'number'
          then (e.value ->> 'opportunities')::float8 end as opps,
        case when jsonb_typeof(e.value -> 'misses') = 'number'
          then (e.value ->> 'misses')::float8 end as misses
    ) x
    cross join lateral (
      select greatest(0, x.opps) as n,
        greatest(0, greatest(0, x.opps) - least(greatest(0, x.opps), greatest(0, x.misses)))
          as found
    ) y
    where gm.account_id = ${scope.accountId} and gm.time_class = ${scope.timeClass}::time_class
      and gm.played_at >= ${since.toISOString()}::timestamptz
      and x.opps is not null and x.misses is not null`)) as unknown as Record<string, unknown>[];
  return {
    drilled: rateOf(Number(r?.df ?? 0), Number(r?.dn ?? 0)),
    held: rateOf(Number(r?.hf ?? 0), Number(r?.hn ?? 0)),
  };
}

const minDate = (ds: readonly Date[]): Date | null =>
  ds.length ? new Date(Math.min(...ds.map((d) => d.getTime()))) : null;

/**
 * The train page: today's session, the counts and the efficacy panel, read only. New positions
 * are made by `refreshItems` on the page's POST; `stale` says they are waiting. Null when the
 * scope is not the user's.
 */
export async function loadTrainPage(
  db: Db,
  userId: string,
  scope: CoachScope,
  o: { source: SourceKey | null } & Clock,
): Promise<TrainPage | null> {
  return readOnly(db, async (tx) => {
    if (!(await owns(tx, userId, scope.accountId))) return null;
    const timeZone = o.timeZone ?? "UTC";
    const today = dayOf(o.now, timeZone);
    const stale = await isStale(tx, userId, scope);
    const items = await storedItems(tx, userId, scope);
    const attempts = await attemptsByItem(tx, userId, scope, startOfDay(today, timeZone));
    const attemptedToday = attempts.filter((a) => a.todayN > 0).length;
    // Only the first attempt of each item a day is recorded, so this is every attempt today.
    const rightToday = attempts.reduce((n, a) => n + a.todayRight, 0);
    const introducedToday = items.filter((i) => i.introducedOn === today).length;
    const picked = buildSession(items.map(queueItem), {
      today,
      attemptedToday,
      introducedToday,
      seed: userId,
      ...(o.source ? { source: o.source } : {}),
    });
    const byId = new Map(items.map((i) => [i.id, i]));
    const pickedItems = picked.flatMap((id) => {
      const i = byId.get(id);
      return i ? [i] : [];
    });
    const prevBy = await previousMoves(tx, pickedItems);
    const session = pickedItems.map((i): TrainItem => {
      const last = prevBy.get(`${i.gameId}#${i.ply - 1}`);
      return {
        id: i.id,
        fen: i.fen,
        orientation: (i.fen.split(" ")[1] ?? "w") === "w" ? "white" : "black",
        lastMove: last && UCI.test(last) ? { from: last.slice(0, 2), to: last.slice(2, 4) } : null,
        label: moveLabel(i.fen, i.ply),
      };
    });
    const tomorrow = addDays(today, 1);
    const live = items.filter((i) => !i.heldOut && i.retiredAt === null);
    const dueTomorrow = live.filter(
      (i) => i.introducedOn !== null && i.dueOn !== null && i.dueOn <= tomorrow,
    ).length;

    // Efficacy: per source, the weakness rate before and after its first drill, summed in SQL.
    const firstBy = new Map(attempts.map((a) => [a.itemId, a.firstAt]));
    const own = SOURCES.flatMap((source) => {
      const mine = items.filter((i) => i.sourceKeys.includes(source));
      if (mine.length === 0) return [];
      const firsts = mine.flatMap((i) => {
        const f = firstBy.get(i.id);
        return f ? [f] : [];
      });
      return [{ source, drilled: firsts.length, first: minDate(firsts) }];
    });
    const splits = await weaknessSplits(
      tx,
      scope,
      own.filter((s) => s.source !== "blunder"),
    );
    const blunders = own.find((s) => s.source === "blunder");
    const blunderRow = blunders ? await blunderSplit(tx, scope, blunders.first) : null;
    const rows: EfficacyRow[] = own.map((s) => {
      const split = s.source === "blunder" ? blunderRow : splits.get(s.source);
      const before = rateOf(split?.before.events ?? 0, split?.before.chances ?? 0);
      const after = rateOf(split?.after.events ?? 0, split?.after.chances ?? 0);
      return {
        source: s.source,
        drilled: s.drilled,
        firstDrill: s.first,
        before,
        after,
        verdict: verdictOf(before, after),
      };
    });
    const since = minDate([...firstBy.values()]);
    const drilledTags = new Set(items.filter((i) => firstBy.has(i.id)).flatMap((i) => i.tags));
    const heldTags = new Set(
      items.filter((i) => i.heldOut).flatMap((i) => i.tags.filter((t) => !drilledTags.has(t))),
    );
    const found = since
      ? await patternRates(tx, scope, since, [...drilledTags], [...heldTags])
      : { drilled: rateOf(0, 0), held: rateOf(0, 0) };
    return {
      source: o.source,
      stale,
      session,
      doneToday: attemptedToday,
      rightToday,
      dueTomorrow,
      counts: {
        active: live.filter((i) => i.introducedOn !== null).length,
        fresh: live.filter((i) => i.introducedOn === null).length,
        heldOut: items.filter((i) => i.heldOut).length,
        retired: items.filter((i) => i.retiredAt !== null).length,
      },
      efficacy: {
        rows,
        patterns: {
          since,
          drilled: found.drilled,
          heldOut: found.held,
          verdict: verdictOf(found.held, found.drilled),
        },
      },
    };
  });
}

/**
 * The overview's "Practise today" (ADR 0092): today's session size for the scope, read only.
 * When new games landed since the items were made, it counts the positions the next refresh
 * will add (`candidates`, the trainer's own source), so it agrees with the trainer before the
 * trainer has made them; `stale` lets the page ask for them. A scope with no position at all is
 * a first run. Null when the scope is not the user's.
 */
export async function loadPractiseToday(
  db: Db,
  userId: string,
  scope: Scope,
  o: { source: SourceKey | null } & Clock,
): Promise<(PractiseToday & { stale: boolean }) | null> {
  return readOnly(db, async (tx) => {
    if (!(await owns(tx, userId, scope.accountId))) return null;
    const timeZone = o.timeZone ?? "UTC";
    const today = dayOf(o.now, timeZone);
    const stale = await isStale(tx, userId, scope);
    const items = await storedItems(tx, userId, scope);
    const pending = stale ? (await candidates(tx, userId, scope)).map(pendingItem) : [];
    const attemptedToday =
      items.length === 0 ? 0 : await attemptedSince(tx, userId, scope, startOfDay(today, timeZone));
    const p = practiseToday([...items.map(queueItem), ...pending], {
      today,
      attemptedToday,
      introducedToday: items.filter((i) => i.introducedOn === today).length,
      seed: userId,
      source: o.source,
    });
    return { ...p, stale };
  });
}

/**
 * The drillable positions of one source for the weakness page's button: the stored ones, and
 * those the next refresh will add (read only).
 */
export async function practiseCount(
  db: Db,
  userId: string,
  scope: CoachScope,
  source: SourceKey,
): Promise<number> {
  return readOnly(db, async (tx) => {
    if (!(await owns(tx, userId, scope.accountId))) return 0;
    const [row] = await tx
      .select({ n: sql<number>`count(*)::int` })
      .from(drillItems)
      .where(
        and(
          eq(drillItems.userId, userId),
          eq(drillItems.accountId, scope.accountId),
          eq(drillItems.timeClass, scope.timeClass),
          eq(drillItems.heldOut, false),
          isNull(drillItems.retiredAt),
          sql`${source}::text = any(${drillItems.sourceKeys})`,
        ),
      );
    const pending = (await pendingItems(tx, userId, scope)).filter(
      (i) => !i.heldOut && i.sourceKeys.includes(source),
    );
    return Number(row?.n ?? 0) + pending.length;
  });
}

// ---- Attempts ----

export type LineMove = { san: string; uci: string; color: "white" | "black" };

export type AttemptResult = {
  correct: boolean;
  /** False when the item was already attempted today: graded, but the schedule is unchanged. */
  recorded: boolean;
  feedback: string;
  move: { uci: string; san: string };
  solution: { uci: string; san: string };
  line: LineMove[];
  /** The move played in the game. */
  played: { uci: string; san: string };
  game: { id: string; ply: number; opponent: string; playedAt: string };
  /** The item's source, named only after the attempt (interleaving). */
  sourceKey: SourceKey;
  dueOn: string | null;
  retired: boolean;
};

function lineOf(fen: string, uci: readonly string[]): LineMove[] {
  const out: LineMove[] = [];
  let chess: Chess;
  try {
    chess = new Chess(fen);
  } catch {
    return out;
  }
  for (const u of uci) {
    if (!UCI.test(u)) break;
    try {
      const m = chess.move({ from: u.slice(0, 2), to: u.slice(2, 4), promotion: u[4] });
      out.push({
        san: m.san,
        uci: `${m.from}${m.to}${m.promotion ?? ""}`,
        color: m.color === "w" ? "white" : "black",
      });
    } catch {
      break;
    }
  }
  return out;
}

/** Whether `uci` is a legal move in `fen`. */
function legal(fen: string, uci: string): boolean {
  return lineOf(fen, [uci]).length === 1;
}

/**
 * Grades one attempt at the user's item and records it with the new schedule (the first attempt
 * of the player's day, in `timeZone`, only; the item's row lock makes a concurrent second one wait
 * and find it attempted). Null for an item that is not the user's, is held out or retired, or an illegal
 * move.
 */
export async function recordAttempt(
  db: Db,
  userId: string,
  a: { itemId: string; uci: string; confidence: Confidence; ms: number | null },
  now: Date,
  timeZone = "UTC",
): Promise<AttemptResult | null> {
  const uci = a.uci.toLowerCase();
  return db.transaction(async (t) => {
    const tx = t as unknown as Db;
    const [item] = await tx
      .select({
        item: drillItems,
        opponent: sql<string>`case when ${games.playerColor} = 'white' then ${games.blackName} else ${games.whiteName} end`,
        playedAt: games.playedAt,
      })
      .from(drillItems)
      .innerJoin(games, eq(games.id, drillItems.gameId))
      .where(and(eq(drillItems.id, a.itemId), eq(drillItems.userId, userId)))
      .for("update", { of: drillItems });
    if (!item || item.item.heldOut || item.item.retiredAt !== null) return null;
    if (!(await owns(tx, userId, item.item.accountId))) return null;
    const it = item.item;
    if (!legal(it.fen, uci)) return null;
    const correct = grade(it.acceptedUci, uci);
    const today = dayOf(now, timeZone);
    const recorded = it.lastAttemptOn !== today;
    let dueOn = it.dueOn;
    let retired = false;
    if (recorded) {
      const next = afterAttempt(
        {
          streak: it.streak,
          cleanPasses: it.cleanPasses,
          dueOn: it.dueOn,
          introducedOn: it.introducedOn,
          lastAttemptOn: it.lastAttemptOn,
          retired: false,
        },
        { correct, confidence: a.confidence, today },
      );
      await tx
        .update(drillItems)
        .set({
          streak: next.streak,
          cleanPasses: next.cleanPasses,
          dueOn: next.dueOn,
          introducedOn: next.introducedOn,
          lastAttemptOn: next.lastAttemptOn,
          retiredAt: next.retired ? now : null,
        })
        .where(eq(drillItems.id, it.id));
      await tx.insert(drillAttempts).values({
        itemId: it.id,
        userId,
        at: now,
        correct,
        confidence: a.confidence,
        moveUci: uci,
        ms: a.ms,
      });
      dueOn = next.dueOn;
      retired = next.retired;
    }
    const line = lineOf(it.fen, it.solutionLine.length ? it.solutionLine : [it.solutionUci]);
    return {
      correct,
      recorded,
      feedback: feedback({ correct, fen: it.fen, moveUci: uci, solutionUci: it.solutionUci }),
      move: { uci, san: sanOf(it.fen, uci) },
      solution: { uci: it.solutionUci, san: sanOf(it.fen, it.solutionUci) },
      line,
      played: { uci: it.playedUci, san: sanOf(it.fen, it.playedUci) },
      game: {
        id: it.gameId,
        ply: it.ply,
        opponent: item.opponent,
        playedAt: item.playedAt.toISOString(),
      },
      sourceKey: isSourceKey(it.sourceKey) ? it.sourceKey : "blunder",
      dueOn,
      retired,
    };
  });
}
