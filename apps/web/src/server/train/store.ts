import { fenKey } from "@ci/chess-core";
import type { Db } from "@ci/db";
import { drillAttempts, drillItems, games, moves } from "@ci/db/generated";
import { Chess } from "chess.js";
import { and, eq, gte, inArray, isNull, sql } from "drizzle-orm";
import { readOnly, scopedAccountIds } from "../games";
import type { CoachScope } from "../profile/types";
import {
  acceptedSet,
  addDays,
  afterAttempt,
  beforeAfter,
  buildSession,
  type Candidate,
  type Confidence,
  dayOf,
  feedback,
  grade,
  isHeldOut,
  isSourceKey,
  keepCandidate,
  type MotifCounts,
  type MultipvLine,
  patternFound,
  type PractiseToday,
  practiseToday,
  primarySource,
  type QueueItem,
  type Rate,
  SOURCES,
  type SourceKey,
  sanOf,
  type Verdict,
  verdictOf,
  WEAKNESS_SOURCES,
} from "./model";

// The trainer's rows (ADR 0090): items are made from the scope's stored analysis on each visit
// (new positions only, never rewritten), served by `buildSession`, and graded against the stored
// accepted set. Every read and write is the user's own: items carry `user_id`, and the scope's
// account must be one of the user's links.

/** The newest candidate positions read per visit. */
export const MAX_CANDIDATES = 400;

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

/**
 * Adds the scope's new drillable positions for the user: the player's own moves at a weakness
 * exemplar ply (`W4.missed`, `W3.*`, `W7.traps`) or judged a blunder, kept by `keepCandidate`.
 * Existing items are never touched. Returns how many were added.
 */
export async function refreshItems(db: Db, userId: string, scope: CoachScope): Promise<number> {
  return db.transaction(async (t) => {
    const tx = t as unknown as Db;
    if (!(await owns(tx, userId, scope.accountId))) return 0;
    const keys = sql.join(
      WEAKNESS_SOURCES.map((k) => sql`${k}::text`),
      sql`, `,
    );
    const rows = (await tx.execute(sql`
      select c.*, pm.san as prev_san, f.best_pv, f.best_line, f.multipv
      from (
        select g.id as game_id, g.played_at, m.ply, m.fen_before, m.uci, m.best_uci,
          m.win_before, m.win_after, m.judgement::text as judgement,
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
          and not exists (
            select 1 from drill_items d
            where d.user_id = ${userId} and d.game_id = g.id and d.ply = m.ply)
      ) c
      left join moves pm on pm.game_id = c.game_id and pm.ply = c.ply - 1
      left join move_features f on f.game_id = c.game_id and f.ply = c.ply
      where c.judgement = 'blunder' or cardinality(c.w_sources) > 0
      order by c.played_at desc, c.ply
      limit ${MAX_CANDIDATES}`)) as unknown as Record<string, unknown>[];
    if (rows.length === 0) return 0;

    const kept = rows.flatMap((r) => {
      const winBefore = num(r.win_before);
      const winAfter = num(r.win_after);
      if (!fin(winBefore) || !fin(winAfter)) return [];
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
      return keepCandidate(c) ? [{ c, r }] : [];
    });
    if (kept.length === 0) return 0;

    // The alternatives: the stored MultiPV of each position (any protocol, the newest), else
    // the move's own copy in `move_features.multipv`.
    const fenKeys = [...new Set(kept.map((k) => fenKey(k.c.fen)))];
    const mpv = (await tx.execute(sql`
      select distinct on (fen_key) fen_key, lines from position_multipv
      where fen_key in (${sql.join(
        fenKeys.map((k) => sql`${k}`),
        sql`, `,
      )})
      order by fen_key, created_at desc`)) as unknown as { fen_key: string; lines: unknown }[];
    const linesBy = new Map(mpv.map((m) => [m.fen_key, parseLines(m.lines)]));

    const values = kept.map(({ c, r }) => {
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
    const added = await tx
      .insert(drillItems)
      .values(values)
      .onConflictDoNothing()
      .returning({ id: drillItems.id });
    return added.length;
  });
}

// ---- Reads ----

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
  session: TrainItem[];
  doneToday: number;
  rightToday: number;
  dueTomorrow: number;
  counts: { active: number; fresh: number; heldOut: number; retired: number };
  efficacy: { rows: EfficacyRow[]; patterns: PatternCompare };
};

function moveLabel(fen: string, ply: number): string {
  const white = (fen.split(" ")[1] ?? "w") === "w";
  return `Move ${Math.ceil(ply / 2)}, ${white ? "White" : "Black"} to move`;
}

const startOfDay = (day: string) => new Date(`${day}T00:00:00Z`);

/**
 * The train page: new positions added first (`refreshItems`), then today's session, the counts,
 * and the efficacy panel. Null when the scope is not the user's.
 */
export async function loadTrainPage(
  db: Db,
  userId: string,
  scope: CoachScope,
  o: { source: SourceKey | null; now: Date },
): Promise<TrainPage | null> {
  await refreshItems(db, userId, scope);
  return readOnly(db, async (tx) => {
    if (!(await owns(tx, userId, scope.accountId))) return null;
    const today = dayOf(o.now);
    const items = await tx
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
    const ids = items.map((i) => i.id);
    const attempts =
      ids.length === 0
        ? []
        : await tx
            .select({
              itemId: drillAttempts.itemId,
              at: drillAttempts.at,
              correct: drillAttempts.correct,
            })
            .from(drillAttempts)
            .where(and(eq(drillAttempts.userId, userId), inArray(drillAttempts.itemId, ids)));
    const todayStart = startOfDay(today);
    const todays = attempts.filter((a) => a.at >= todayStart);
    const attemptedToday = new Set(todays.map((a) => a.itemId)).size;
    // The first attempt of each item today (only it is recorded, so this is every attempt).
    const rightToday = todays.filter((a) => a.correct).length;
    const introducedToday = items.filter((i) => i.introducedOn === today).length;
    const queue: QueueItem[] = items.map((i) => ({
      id: i.id,
      sourceKey: isSourceKey(i.sourceKey) ? i.sourceKey : "blunder",
      sourceKeys: i.sourceKeys.filter(isSourceKey),
      dueOn: i.dueOn,
      introducedOn: i.introducedOn,
      heldOut: i.heldOut,
      retired: i.retiredAt !== null,
      playedAt: i.playedAt,
    }));
    const picked = buildSession(queue, {
      today,
      attemptedToday,
      introducedToday,
      seed: userId,
      ...(o.source ? { source: o.source } : {}),
    });
    const byId = new Map(items.map((i) => [i.id, i]));
    const prev = picked.length
      ? await tx
          .select({ gameId: moves.gameId, ply: moves.ply, uci: moves.uci })
          .from(moves)
          .where(
            and(
              inArray(moves.gameId, [...new Set(picked.map((id) => byId.get(id)?.gameId ?? ""))]),
              inArray(moves.ply, [...new Set(picked.map((id) => (byId.get(id)?.ply ?? 1) - 1))]),
            ),
          )
      : [];
    const prevBy = new Map(prev.map((p) => [`${p.gameId}#${p.ply}`, p.uci]));
    const session = picked.flatMap((id): TrainItem[] => {
      const i = byId.get(id);
      if (!i) return [];
      const last = prevBy.get(`${i.gameId}#${i.ply - 1}`);
      return [
        {
          id,
          fen: i.fen,
          orientation: (i.fen.split(" ")[1] ?? "w") === "w" ? "white" : "black",
          lastMove:
            last && UCI.test(last) ? { from: last.slice(0, 2), to: last.slice(2, 4) } : null,
          label: moveLabel(i.fen, i.ply),
        },
      ];
    });
    const tomorrow = addDays(today, 1);
    const live = items.filter((i) => !i.heldOut && i.retiredAt === null);
    const dueTomorrow = live.filter(
      (i) => i.introducedOn !== null && i.dueOn !== null && i.dueOn <= tomorrow,
    ).length;

    // Efficacy: per source, the weakness rate before and after its first drill.
    const firstBy = new Map<string, Date>();
    for (const a of attempts) {
      const prevFirst = firstBy.get(a.itemId);
      if (!prevFirst || a.at < prevFirst) firstBy.set(a.itemId, a.at);
    }
    const metricsRows = await tx.execute(sql`
      select gm.played_at, gm.metrics -> 'weakness' as weakness, gm.metrics -> 'missedByMotif' as motifs
      from game_metrics gm
      where gm.account_id = ${scope.accountId} and gm.time_class = ${scope.timeClass}::time_class
      order by gm.played_at`);
    const metrics = (metricsRows as unknown as Record<string, unknown>[]).map((r) => ({
      playedAt: r.played_at instanceof Date ? r.played_at : new Date(String(r.played_at)),
      weakness: (typeof r.weakness === "object" && r.weakness !== null ? r.weakness : {}) as Record<
        string,
        unknown
      >,
      motifs: (typeof r.motifs === "object" && r.motifs !== null ? r.motifs : {}) as MotifCounts,
    }));
    const blunderRows = (await tx.execute(sql`
      select g.played_at, count(*) filter (where m.judgement = 'blunder')::int as events,
        count(*)::int as chances
      from games g join moves m on m.game_id = g.id and m.color = g.player_color
      where g.account_id = ${scope.accountId} and g.time_class = ${scope.timeClass}::time_class
      group by g.id, g.played_at`)) as unknown as Record<string, unknown>[];
    const rows: EfficacyRow[] = SOURCES.flatMap((source): EfficacyRow[] => {
      const own = items.filter((i) => i.sourceKeys.includes(source));
      if (own.length === 0) return [];
      const firsts = own.flatMap((i) => {
        const f = firstBy.get(i.id);
        return f ? [f] : [];
      });
      const firstDrill = firsts.length
        ? new Date(Math.min(...firsts.map((d) => d.getTime())))
        : null;
      const perGame =
        source === "blunder"
          ? blunderRows.map((r) => ({
              playedAt: r.played_at instanceof Date ? r.played_at : new Date(String(r.played_at)),
              events: Number(r.events),
              chances: Number(r.chances),
            }))
          : metrics.flatMap((m) => {
              const w = m.weakness[source];
              if (typeof w !== "object" || w === null) return [];
              const { events, opportunities } = w as Record<string, unknown>;
              if (!fin(events) || !fin(opportunities) || opportunities <= 0) return [];
              return [{ playedAt: m.playedAt, events, chances: opportunities }];
            });
      return [{ source, drilled: firsts.length, firstDrill, ...beforeAfter(perGame, firstDrill) }];
    });
    const allFirsts = [...firstBy.values()];
    const since = allFirsts.length
      ? new Date(Math.min(...allFirsts.map((d) => d.getTime())))
      : null;
    const drilledTags = new Set(items.filter((i) => firstBy.has(i.id)).flatMap((i) => i.tags));
    const heldTags = new Set(
      items.filter((i) => i.heldOut).flatMap((i) => i.tags.filter((t) => !drilledTags.has(t))),
    );
    const games2 = metrics.map((m) => ({ playedAt: m.playedAt, byMotif: m.motifs }));
    const far = new Date(8.64e15);
    const drilled = patternFound(games2, drilledTags, since ?? far);
    const held = patternFound(games2, heldTags, since ?? far);
    return {
      source: o.source,
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
        patterns: { since, drilled, heldOut: held, verdict: verdictOf(held, drilled) },
      },
    };
  });
}

/**
 * The overview's "Practise today" (ADR 0092): today's session size for the scope, read only. It
 * adds no items (the trainer does on its visit), so a scope with none yet is a first run. Null
 * when the scope is not the user's.
 */
export async function loadPractiseToday(
  db: Db,
  userId: string,
  scope: Pick<CoachScope, "accountId" | "timeClass">,
  o: { source: SourceKey | null; now: Date },
): Promise<PractiseToday | null> {
  return readOnly(db, async (tx) => {
    if (!(await owns(tx, userId, scope.accountId))) return null;
    const today = dayOf(o.now);
    const items = await tx
      .select({
        id: drillItems.id,
        sourceKey: drillItems.sourceKey,
        sourceKeys: drillItems.sourceKeys,
        dueOn: drillItems.dueOn,
        introducedOn: drillItems.introducedOn,
        heldOut: drillItems.heldOut,
        retiredAt: drillItems.retiredAt,
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
    const ids = items.map((i) => i.id);
    const [row] =
      ids.length === 0
        ? [{ n: 0 }]
        : await tx
            .select({ n: sql<number>`count(distinct ${drillAttempts.itemId})::int` })
            .from(drillAttempts)
            .where(
              and(
                eq(drillAttempts.userId, userId),
                inArray(drillAttempts.itemId, ids),
                gte(drillAttempts.at, startOfDay(today)),
              ),
            );
    const queue: QueueItem[] = items.map((i) => ({
      id: i.id,
      sourceKey: isSourceKey(i.sourceKey) ? i.sourceKey : "blunder",
      sourceKeys: i.sourceKeys.filter(isSourceKey),
      dueOn: i.dueOn,
      introducedOn: i.introducedOn,
      heldOut: i.heldOut,
      retired: i.retiredAt !== null,
      playedAt: i.playedAt,
    }));
    return practiseToday(queue, {
      today,
      attemptedToday: Number(row?.n ?? 0),
      introducedToday: items.filter((i) => i.introducedOn === today).length,
      seed: userId,
      source: o.source,
    });
  });
}

/** The drillable positions of one source for the weakness page's button (new ones added first). */
export async function practiseCount(
  db: Db,
  userId: string,
  scope: CoachScope,
  source: SourceKey,
): Promise<number> {
  await refreshItems(db, userId, scope);
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
    return Number(row?.n ?? 0);
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
 * of a day only). Null for an item that is not the user's, is held out or retired, or an illegal
 * move.
 */
export async function recordAttempt(
  db: Db,
  userId: string,
  a: { itemId: string; uci: string; confidence: Confidence; ms: number | null },
  now: Date,
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
    const today = dayOf(now);
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
