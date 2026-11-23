import { readFileSync } from "node:fs";
import { accountLinks, type Db, games, linkedAccounts, moves, user } from "@ci/db";
import { gameMetrics, moveFeatures, openingLines } from "@ci/db/generated";
import { games as fixtures, type GameFixture } from "@ci/ui/fixtures";
import { Chess } from "chess.js";
import { and, eq, inArray, sql } from "drizzle-orm";
import { DEMO_USERNAME_PREFIX } from "../src/server/demo";
import { resolveCoachScope } from "../src/server/profile";
import { refreshItems } from "../src/server/train/store";
import { seedNote, seedReport, seedWeekly } from "../test/helpers/coaching";
import { asDemoSeed, seedFixtureGame, seedSyntheticGame } from "../test/helpers/games";
import { seedProfile } from "../test/helpers/profile";

// `pnpm seed:demo` (ADR 0097): a fictional player's analysed games in the local dev database, so
// every page has content offline and without a chess.com account. It reuses the synthetic worlds
// of the e2e seeds (e2e/helpers/seed-games.ts, seed-coach.ts, seed-strengths.ts) and their test
// writers, but only ever touches the demo player's own rows: the database may hold real accounts.
// The demo player's username has a dot, which no platform allows, so the worker never syncs it
// and the app shows a "Demo data" badge. Every run removes the demo and writes it again (fixed
// ids, dates moved by whole weeks so the newest game is from the last seven days).

export const DEMO_USERNAME = `${DEMO_USERNAME_PREFIX}player`;
const DEMO_NAME = "Demo Player";
const TC = "rapid" as const;
const DAY = 86_400_000;
const INPUT = new URL(
  "../../../backend/fixtures/ai/input/coaching-input.b2-rapid.json",
  import.meta.url,
);

/** Fixed ids (`de` marks the demo), so a second run writes the same rows. */
const idOf = (n: number) => `00000000-0000-4000-8000-de${String(n).padStart(10, "0")}`;
const ACCOUNT_ID = idOf(0);

/** The newest game of the e2e worlds; every date moves by whole weeks from it. */
const ANCHOR = Date.UTC(2026, 9, 31, 18);

function weekShift(now: number): number {
  return Math.floor((now - ANCHOR) / (7 * DAY)) * 7 * DAY;
}

/** ISO week key, `2026-W47`. */
export function isoWeek(d: Date): string {
  const t = new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()));
  t.setUTCDate(t.getUTCDate() + 4 - (t.getUTCDay() || 7));
  const year = t.getUTCFullYear();
  const week = Math.ceil(((t.getTime() - Date.UTC(year, 0, 1)) / DAY + 1) / 7);
  return `${year}-W${String(week).padStart(2, "0")}`;
}

const ymd = (d: Date) => d.toISOString().slice(0, 10);

/** The fixture with fictional names and club ratings: no real player's name in the demo. */
function fictional(f: GameFixture, opponent: string, playerColor: "white" | "black"): GameFixture {
  const me = { name: DEMO_NAME, rating: 1520 };
  const them = { name: opponent, rating: 1505 };
  return {
    ...f,
    source: "Rematch demo",
    playerColor,
    white: playerColor === "white" ? me : them,
    black: playerColor === "black" ? me : them,
  };
}

async function demoAccounts(db: Db): Promise<string[]> {
  const rows = (await db.execute(
    sql`select id from linked_accounts where starts_with(username, ${DEMO_USERNAME_PREFIX})`,
  )) as unknown as { id: string }[];
  return rows.map((r) => r.id);
}

/** Removes the demo player and every row that hangs off it. The user (a real login) stays. */
export async function removeDemo(db: Db): Promise<number> {
  const ids = await demoAccounts(db);
  for (const id of ids) {
    await db.execute(sql`delete from user_events where account_id = ${id}`);
    await db.execute(sql`delete from jobs where user_key = ${id}`);
    await db.execute(
      sql`delete from move_features where game_id in (select id from games where account_id = ${id})`,
    );
    await db.execute(sql`update report_runs set report_id = null where account_id = ${id}`);
    await db.execute(
      sql`delete from report_sections where report_id in (select id from reports where account_id = ${id})`,
    );
    await db.execute(sql`delete from reports where account_id = ${id}`);
    // Every other table cascades from linked_accounts.
    await db.execute(sql`delete from linked_accounts where id = ${id}`);
  }
  return ids.length;
}

async function ensureUser(db: Db, email: string): Promise<string> {
  const [have] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  if (have) return have.id;
  const [made] = await db
    .insert(user)
    .values({ email, name: email.split("@")[0] ?? email })
    .returning({ id: user.id });
  if (!made) throw new Error("seed:demo: could not create the user");
  return made.id;
}

type Rapid = { id: string; playedAt: Date };

async function seedGames(db: Db, shift: number): Promise<{ rapid: Rapid[] }> {
  const at = (iso: string) => new Date(Date.parse(iso) + shift);
  await seedFixtureGame(
    db,
    ACCOUNT_ID,
    fictional(fixtures.middlegameBlunder, "Opponent A", "black"),
    {
      id: idOf(1),
      playedAt: at("2026-10-31T18:00:00Z"),
      platformGameId: "demo-1",
    },
  );
  await seedFixtureGame(db, ACCOUNT_ID, fictional(fixtures.endgameGrind, "Opponent B", "white"), {
    id: idOf(2),
    playedAt: at("2026-10-30T18:00:00Z"),
    platformGameId: "demo-2",
  });
  await seedFixtureGame(db, ACCOUNT_ID, fictional(fixtures.shortAttack, "Opponent C", "black"), {
    id: idOf(3),
    playedAt: at("2026-10-29T18:00:00Z"),
    platformGameId: "demo-3",
  });
  // An engine move at ply 4 of the newest game, so "Show the engine's next move" has an arrow.
  const [fourth] = await db
    .select({ fen: moves.fenBefore, uci: moves.uci })
    .from(moves)
    .where(and(eq(moves.gameId, idOf(1)), eq(moves.ply, 4)));
  if (fourth) {
    const other = new Chess(fourth.fen)
      .moves({ verbose: true })
      .map((m) => `${m.from}${m.to}${m.promotion ?? ""}`)
      .find((u) => u !== fourth.uci);
    await db
      .update(moves)
      .set({ bestUci: other ?? null })
      .where(and(eq(moves.gameId, idOf(1)), eq(moves.ply, 4)));
  }
  const results = ["1-0", "0-1", "1/2-1/2", "0-1"] as const;
  const rapid: Rapid[] = [];
  for (let i = 0; i < 30; i++) {
    const playedAt = new Date(Date.UTC(2026, 8, 1, 10 + (i % 3)) + i * 2 * DAY + shift);
    const id = idOf(10 + i);
    await seedSyntheticGame(db, ACCOUNT_ID, {
      id,
      playedAt: playedAt.toISOString(),
      timeClass: i % 2 === 0 ? "rapid" : "blitz",
      result: results[i % 4] ?? "1-0",
      playerColor: i % 3 === 0 ? "black" : "white",
      accuracy: 45 + ((i * 17) % 45),
      counts: { blunders: i % 4, mistakes: (i * 3) % 4, inaccuracies: i % 3 },
      opponentRating: 1200 + ((i * 41) % 600),
      baseSeconds: i % 2 === 0 ? 600 : 180,
      incrementSeconds: i % 2 === 0 ? 0 : 2,
    });
    if (i % 2 === 0) rapid.push({ id, playedAt });
  }
  await db
    .update(games)
    .set({ whiteName: DEMO_NAME })
    .where(and(eq(games.accountId, ACCOUNT_ID), eq(games.whiteName, "me")));
  await db
    .update(games)
    .set({ blackName: DEMO_NAME })
    .where(and(eq(games.accountId, ACCOUNT_ID), eq(games.blackName, "me")));
  await db
    .update(games)
    .set({ whiteName: "Opponent" })
    .where(and(eq(games.accountId, ACCOUNT_ID), eq(games.whiteName, "opp")));
  await db
    .update(games)
    .set({ blackName: "Opponent" })
    .where(and(eq(games.accountId, ACCOUNT_ID), eq(games.blackName, "opp")));
  return { rapid };
}

/** The player's first blunder in each of the three newest rapid games, with an engine move. */
async function examplePlies(db: Db, rapid: Rapid[]): Promise<Map<string, number>> {
  type Row = { game_id: string; ply: number; fen: string; uci: string };
  const rows = (await db.execute(sql`
    select distinct on (m.game_id) m.game_id, m.ply, m.fen_before as fen, m.uci
    from moves m join games g on g.id = m.game_id
    where g.id in (${sql.join(
      rapid.map((g) => sql`${g.id}::uuid`),
      sql`, `,
    )}) and m.color = g.player_color and m.judgement = 'blunder'
    order by m.game_id, m.ply`)) as unknown as Row[];
  const byGame = new Map(rows.map((r) => [r.game_id, r]));
  const wins = [
    [62, 31],
    [55, 24],
    [70, 38],
  ] as const;
  const out = new Map<string, number>();
  const newest = [...rapid]
    .reverse()
    .filter((g) => byGame.has(g.id))
    .slice(0, 3);
  for (const [i, g] of newest.entries()) {
    const r = byGame.get(g.id);
    if (!r) continue;
    const [before, after] = wins[i] ?? [60, 30];
    const best = new Chess(r.fen)
      .moves({ verbose: true })
      .map((m) => `${m.from}${m.to}${m.promotion ?? ""}`)
      .find((u) => u !== r.uci);
    await db
      .update(moves)
      .set({ winBefore: before, winAfter: after, deltaW: before - after, bestUci: best ?? null })
      .where(and(eq(moves.gameId, g.id), eq(moves.ply, r.ply)));
    out.set(g.id, r.ply);
  }
  return out;
}

async function seedCoachWorld(db: Db, rapid: Rapid[], shift: number): Promise<void> {
  const at = (day: string) => new Date(Date.parse(`${day}T12:00:00Z`) + shift);
  const examples = await examplePlies(db, rapid);
  for (const g of rapid) {
    const ply = examples.get(g.id);
    await db.insert(gameMetrics).values({
      gameId: g.id,
      accountId: ACCOUNT_ID,
      timeClass: TC,
      playedAt: g.playedAt,
      protocol: "demo-1",
      metrics:
        ply === undefined
          ? { version: 1 }
          : { version: 1, weakness: { "W3.shallow": { exemplarPlies: [ply] } } },
    });
  }
  const est = (
    seriesKey: string,
    state: string,
    priority: number,
    k: number,
    n: number,
    mean: number,
    lo: number,
    hi: number,
  ) => ({
    seriesKey,
    state: state as "active",
    priority,
    k,
    n,
    kUnweighted: k,
    nUnweighted: n,
    alpha: k + 1,
    beta: n - k + 1,
    mean,
    ciLow: lo,
    ciHigh: hi,
    bandMedian: mean * 0.7,
    bandP40: mean * 0.6,
    excess: mean * 0.3,
    confidence: 0.85,
  });
  // Twelve weeks ending with this week's Monday, one of them without analysed games.
  const firstMonday = Date.UTC(2026, 7, 31) + shift;
  const weeks = Array.from({ length: 12 }, (_, i) => isoWeek(new Date(firstMonday + i * 7 * DAY)));
  const nullWeek = weeks[5];
  const weekly = (metric: string, base: number, step: number, n: number) =>
    weeks.map((week, i) =>
      week === nullWeek
        ? { metric, windowKey: week, value: null, n: 0 }
        : {
            metric,
            windowKey: week,
            value: base + step * i,
            ciLow: base + step * i - 2,
            ciHigh: base + step * i + 2,
            n,
          },
    );
  const area = (name: string, value: number | null, n: number) => ({
    metric: `area:${name}`,
    windowKey: "last50",
    value,
    ciLow: value === null ? null : value - 6,
    ciHigh: value === null ? null : value + 6,
    n,
  });
  await seedProfile(db, ACCOUNT_ID, {
    timeClass: TC,
    estimates: [
      est("W3.shallow", "active", 9, 22, 120, 0.18, 0.15, 0.22),
      est("W5", "emerging", 7, 9, 40, 0.22, 0.15, 0.3),
      est("W1", "active", 6, 58, 2400, 0.024, 0.021, 0.028),
      est("W8.basics", "improving", 5, 6, 70, 0.09, 0.05, 0.14),
      est("W9a", "relapsed", 4, 7, 30, 0.23, 0.14, 0.33),
      est("W7.traps", "emerging", 3, 2, 12, 0.17, 0.07, 0.32),
      est("W4.missed", "resolved", 2, 3, 90, 0.035, 0.015, 0.07),
      est("W6", "not_an_issue", 1, 1, 50, 0.02, 0.005, 0.06),
    ],
    history: [
      {
        seriesKey: "W3.shallow",
        kind: "transition",
        toState: "emerging",
        atPlayedAt: at("2026-09-03"),
        atGameId: rapid[1]?.id,
      },
      {
        seriesKey: "W3.shallow",
        kind: "transition",
        fromState: "emerging",
        toState: "active",
        atPlayedAt: at("2026-09-25"),
        atGameId: rapid[6]?.id,
        rateBefore: 0.11,
        rateAfter: 0.18,
        evidence: { k: 22, n: 120, confidence: 0.85 },
      },
      {
        seriesKey: "W8.basics",
        kind: "transition",
        toState: "active",
        atPlayedAt: at("2026-09-05"),
        atGameId: rapid[1]?.id,
      },
      {
        seriesKey: "W8.basics",
        kind: "transition",
        fromState: "active",
        toState: "improving",
        atPlayedAt: at("2026-10-15"),
        atGameId: rapid[11]?.id,
        rateBefore: 0.14,
        rateAfter: 0.09,
        pChange: 0.85,
        evidence: { k: 6, n: 70, confidence: 0.82 },
      },
      {
        seriesKey: "W1",
        kind: "transition",
        toState: "active",
        atPlayedAt: at("2026-09-09"),
        atGameId: rapid[2]?.id,
      },
    ],
    points: rapid.flatMap((g, j) => [
      {
        seriesKey: "W3.shallow",
        gameId: g.id,
        playedAt: g.playedAt,
        events: j % 3 === 0 ? 2 : 1,
        opportunities: 8,
        epl: 0.4,
      },
      {
        seriesKey: "W1",
        gameId: g.id,
        playedAt: g.playedAt,
        events: j % 4 === 0 ? 1 : 0,
        opportunities: 30,
        epl: 0.1,
      },
    ]),
    kpis: [
      ...weekly("accuracy", 71, 0.5, 22),
      ...weekly("blunders_per_game", 1.4, -0.03, 22),
      { metric: "accuracy", windowKey: "last50", value: 74.2, ciLow: 72.8, ciHigh: 75.6, n: 40 },
      {
        metric: "blunders_per_game",
        windowKey: "last50",
        value: 1.1,
        ciLow: 0.8,
        ciHigh: 1.4,
        n: 40,
      },
      area("openings", 48, 15),
      area("middlegame", 55, 15),
      area("endgame", 41, 15),
      area("tactics", 37, 15),
      // Too few games for a calculation score: stored null, shown as a dash, never 0.
      area("calculation", null, 6),
      area("time", 60, 15),
    ],
  });

  const line = (
    lineKey: string,
    color: "white" | "black",
    kind: "chosen" | "faced",
    verdict: "keep" | "fix" | "learn",
    n: number,
    summary: Record<string, unknown>,
  ) => ({
    accountId: ACCOUNT_ID,
    timeClass: TC,
    color,
    lineKey,
    kind,
    games: n,
    verdict,
    summary: { lineKey, color, kind, verdict, games: n, struggles: [], share: 0.2, ...summary },
  });
  await db.insert(openingLines).values([
    line("w-italian", "white", "chosen", "keep", 41, {
      perfVsExpected: 0.04,
      perfGames: 41,
      perfInterval: { low: -0.03, high: 0.11 },
    }),
    line("b-sicilian-najdorf", "black", "chosen", "fix", 23, {
      perfVsExpected: -0.09,
      perfGames: 23,
      errorSpot: { fenKey: "demo", uci: "d8c7", san: "Qc7", games: 7 },
      struggles: ["repeated_deviation_error"],
    }),
    line("w-scandi-faced", "white", "faced", "learn", 9, {
      perfVsExpected: null,
      perfGames: 0,
      analysedGames: 9,
      avgWinAtOpeningEnd: 37.6,
      struggles: ["worse_after_opening"],
    }),
  ]);

  // Strengths: quiet good moves of the two fixture games played as Black (seed-strengths.ts).
  const spots = [
    { gameId: idOf(3), ply: 16, gap: 31, kind: "tactic_found" },
    { gameId: idOf(1), ply: 54, gap: 24, kind: "only_move_found" },
    { gameId: idOf(1), ply: 28, gap: 18, kind: "tactic_found" },
  ] as const;
  const strengthGames = await db
    .select({ id: games.id, playedAt: games.playedAt, timeClass: games.timeClass })
    .from(games)
    .where(inArray(games.id, [idOf(1), idOf(3)]));
  for (const g of strengthGames) {
    await db.insert(gameMetrics).values({
      gameId: g.id,
      accountId: ACCOUNT_ID,
      timeClass: g.timeClass,
      playedAt: g.playedAt,
      protocol: "demo-strengths",
      metrics: {
        version: 1,
        strengths: spots
          .filter((s) => s.gameId === g.id)
          .map((s) => ({ kind: s.kind, ply: s.ply })),
      },
    });
  }
  await db.insert(moveFeatures).values(
    spots.map((s) => ({
      gameId: s.gameId,
      ply: s.ply,
      enrichmentProtocol: "demo-strengths",
      difficultyStatus: "pending_maia" as const,
      isCritical: true,
      multipvGap: s.gap,
    })),
  );

  // One complete example report and last week's review: what the coach writes, labelled demo.
  const input = JSON.parse(readFileSync(INPUT, "utf8")) as Record<string, unknown>;
  await seedReport(db, ACCOUNT_ID, {
    timeClass: TC,
    payload: "example",
    createdAt: at("2026-11-02"),
    run: { trigger: "manual", isFirstReport: true, input: { ...input, accountId: ACCOUNT_ID } },
  });
  await seedWeekly(db, ACCOUNT_ID, {
    timeClass: TC,
    weekStart: ymd(new Date(Date.UTC(2026, 10, 9) + shift)),
  });
  await seedNote(db, ACCOUNT_ID, {
    changeKey: "demo-w3-onset",
    seriesKey: "W3.shallow",
    onsetDate: ymd(at("2026-09-25")),
  });
}

export type DemoSeed = { accountId: string; userId: string; games: number; drills: number };

/**
 * Writes the demo player and links `email` (created when it has not signed in yet) to it,
 * verified. Removes any previous demo first, so it is idempotent.
 */
export async function seedDemo(db: Db, opts: { email: string; now?: number }): Promise<DemoSeed> {
  const email = opts.email.trim().toLowerCase();
  if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
    throw new Error(`seed:demo needs an email address, got "${opts.email}"`);
  }
  const shift = weekShift(opts.now ?? Date.now());
  return asDemoSeed(db, async () => {
    await removeDemo(db);
    const userId = await ensureUser(db, email);
    await db.insert(linkedAccounts).values({
      id: ACCOUNT_ID,
      platform: "chesscom",
      username: DEMO_USERNAME,
      displayName: DEMO_NAME,
    });
    await db.insert(accountLinks).values({
      userId,
      linkedAccountId: ACCOUNT_ID,
      method: "username",
      verifiedAt: new Date(),
    });
    const { rapid } = await seedGames(db, shift);
    await seedCoachWorld(db, rapid, shift);
    const scope = await resolveCoachScope(db, userId, { account: ACCOUNT_ID, tc: TC });
    const drills = scope ? await refreshItems(db, userId, scope) : 0;
    const [n] = (await db.execute(
      sql`select count(*)::int as n from games where account_id = ${ACCOUNT_ID}`,
    )) as unknown as { n: number }[];
    return { accountId: ACCOUNT_ID, userId, games: n?.n ?? 0, drills };
  });
}
