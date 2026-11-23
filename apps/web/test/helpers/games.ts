import { randomUUID } from "node:crypto";
import { fenKey, gameAccuracy, moveAccuracy } from "@ci/chess-core";
import {
  accountLinks,
  type Db,
  games,
  linkedAccounts,
  moves,
  type NewGame,
  positionEvals,
} from "@ci/db";
import type { GameFixture } from "@ci/ui/fixtures";
import { Chess } from "chess.js";
import { eq, sql } from "drizzle-orm";
import type { AnalysisStatus, JudgementCounts, TimeClass } from "../../src/server/games/types";

// Test-only writers for the games read model. They write synthetic or public fixture data, and
// every one of them first runs `assertSeedable`, so they cannot touch a real database.

type MoveRow = typeof moves.$inferInsert;

/**
 * Connections of `pnpm seed:demo` (scripts/demo-seed.ts, ADR 0097). It writes the fictional demo
 * player through these helpers into a local dev database, which is not `*_test` and may hold real
 * accounts; it guards itself instead (local host, not production, only its own account's rows).
 */
const demoSeedDbs = new WeakSet<Db>();

export async function asDemoSeed<T>(db: Db, write: () => Promise<T>): Promise<T> {
  demoSeedDbs.add(db);
  try {
    return await write();
  } finally {
    demoSeedDbs.delete(db);
  }
}

/**
 * Refuses unless `db` is a `*_test` database that holds no real or verified accounts: none
 * imported by the worker (`last_synced_at` or `import_cursor` set) and no link verified by a real
 * method (anything but the helpers' own `username` links). It runs before every helper writes.
 */
export async function assertSeedable(db: Db): Promise<void> {
  if (demoSeedDbs.has(db)) return;
  const [current] = (await db.execute(sql`select current_database() as name`)) as unknown as {
    name: string;
  }[];
  const name = current?.name ?? "";
  if (!name.endsWith("_test")) {
    throw new Error(`Refusing to seed: database "${name}" does not end in _test`);
  }
  const [row] = (await db.execute(sql`
    select
      (select count(*) from linked_accounts
        where last_synced_at is not null or import_cursor is not null)::int
      + (select count(*) from account_links
        where verified_at is not null and method <> 'username')::int as real
  `)) as unknown as { real: number }[];
  if ((row?.real ?? 0) > 0) {
    throw new Error(
      `Refusing to seed: database "${name}" holds real or verified accounts (${row?.real})`,
    );
  }
}

/** Empties the game tables (and, by cascade, account links and import runs). */
export async function resetGameTables(db: Db): Promise<void> {
  await assertSeedable(db);
  await db.execute(
    sql`truncate table linked_accounts, games, moves, position_evals, account_links cascade`,
  );
}

/** A linked account, plus the user's `account_links` row (method `username`) when `userId` is set. */
export async function seedAccount(
  db: Db,
  opts: {
    userId?: string;
    platform?: "chesscom" | "lichess";
    username: string;
    verified?: boolean;
  },
): Promise<{ accountId: string }> {
  await assertSeedable(db);
  const [row] = await db
    .insert(linkedAccounts)
    .values({
      platform: opts.platform ?? "chesscom",
      username: opts.username.toLowerCase(),
      displayName: opts.username,
    })
    .returning({ id: linkedAccounts.id });
  if (!row) throw new Error("seedAccount: no row");
  if (opts.userId) {
    await db.insert(accountLinks).values({
      userId: opts.userId,
      linkedAccountId: row.id,
      method: "username",
      // Verified unless the test says otherwise: an unverified link sees analysis only (F10).
      verifiedAt: opts.verified === false ? null : new Date(),
    });
  }
  return { accountId: row.id };
}

async function accountPlatform(db: Db, accountId: string): Promise<"lichess" | "chesscom"> {
  const [row] = await db
    .select({ platform: linkedAccounts.platform })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.id, accountId));
  if (!row) throw new Error(`no linked account ${accountId}`);
  return row.platform;
}

function clk(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function pgnFor(headers: Record<string, string>, sans: string[], clocks: (number | null)[]) {
  const head = Object.entries(headers)
    .map(([k, v]) => `[${k} "${v.replace(/["\\]/g, "\\$&")}"]`)
    .join("\n");
  const body = sans
    .map((san, i) => {
      const num = i % 2 === 0 ? `${i / 2 + 1}. ` : "";
      const c = clocks[i];
      return `${num}${san}${c == null ? "" : ` {[%clk ${clk(c)}]}`}`;
    })
    .join(" ");
  return `${head}\n\n${body} ${headers.Result ?? "*"}\n`;
}

/** The fixtures carry no eval of the start position; it is treated as level (cp 0, win% 50). */
const START_WIN = 50;

/**
 * Writes a public fixture game: the `games` row (a PGN rebuilt from the SAN with `[%clk]`
 * comments, accuracies computed from the fixture's win%), one `moves` row per ply and, by default,
 * a `position_evals` row for every position. Moves and evals default to on only for `done` games.
 */
export async function seedFixtureGame(
  db: Db,
  accountId: string,
  fixture: GameFixture,
  overrides: Partial<NewGame> & {
    withEvals?: boolean;
    withMoves?: boolean;
    protocol?: string;
  } = {},
): Promise<{ gameId: string }> {
  await assertSeedable(db);
  const { withEvals, withMoves, protocol: protocolOverride, ...gameOverrides } = overrides;
  const status = gameOverrides.analysisStatus ?? "done";
  const done = status === "done";
  const protocol = protocolOverride ?? "test-1";
  const platform = gameOverrides.platform ?? (await accountPlatform(db, accountId));
  const winsWhite = [START_WIN, ...fixture.plies.map((p) => p.winWhite)];

  const pgn = pgnFor(
    {
      Event: "Fixture",
      Site: fixture.source,
      White: fixture.white.name,
      Black: fixture.black.name,
      Result: fixture.result,
      ...(fixture.white.rating == null ? {} : { WhiteElo: String(fixture.white.rating) }),
      ...(fixture.black.rating == null ? {} : { BlackElo: String(fixture.black.rating) }),
      ...(fixture.eco ? { ECO: fixture.eco } : {}),
      ...(fixture.openingName ? { Opening: fixture.openingName } : {}),
    },
    fixture.plies.map((p) => p.san),
    fixture.plies.map((p) => p.clockSeconds),
  );

  const [g] = await db
    .insert(games)
    .values({
      accountId,
      platform,
      platformGameId: `${fixture.id}-${randomUUID()}`,
      pgn,
      playedAt: new Date("2026-09-01T12:00:00Z"),
      timeClass: fixture.timeClass as TimeClass,
      rated: true,
      playerColor: fixture.playerColor,
      whiteName: fixture.white.name,
      blackName: fixture.black.name,
      whiteRating: fixture.white.rating,
      blackRating: fixture.black.rating,
      result: fixture.result,
      eco: fixture.eco,
      openingName: fixture.openingName,
      analysisStatus: status,
      analysisProtocol: done ? protocol : null,
      whiteAccuracy: done ? gameAccuracy(winsWhite, "white") : null,
      blackAccuracy: done ? gameAccuracy(winsWhite, "black") : null,
      ...gameOverrides,
    })
    .returning({ id: games.id });
  if (!g) throw new Error("seedFixtureGame: no row");

  if (withMoves ?? done) {
    const rows: MoveRow[] = fixture.plies.map((p, i) => {
      const pov = (w: number) => (p.color === "white" ? w : 100 - w);
      const winBefore = pov(winsWhite[i] as number);
      const winAfter = pov(p.winWhite);
      return {
        gameId: g.id,
        ply: p.ply,
        color: p.color,
        san: p.san,
        uci: p.uci,
        fenBefore: i === 0 ? fixture.startFen : (fixture.plies[i - 1]?.fenAfter as string),
        bestUci: p.bestUci,
        winBefore,
        winAfter,
        deltaW: Math.max(0, winBefore - winAfter),
        accuracy: moveAccuracy(winBefore, winAfter),
        judgement: p.judgement,
        phase: p.phase,
        clockBefore: p.clockSeconds,
        timeSpent: null,
        decided: Math.abs(winBefore - 50) >= 40,
      };
    });
    if (rows.length > 0) await db.insert(moves).values(rows);
  }

  if (withEvals ?? done) {
    const positions = [
      { fen: fixture.startFen, cp: 0 as number | null, mate: null as number | null },
      ...fixture.plies.map((p) => ({ fen: p.fenAfter, cp: p.cpWhite, mate: p.mateWhite })),
    ];
    const rows = positions.map((pos, i) => ({
      fenKey: fenKey(pos.fen),
      protocol,
      scoreCp: pos.mate == null ? pos.cp : null,
      scoreMate: pos.mate,
      bestUci: fixture.plies[i]?.bestUci ?? null,
      source: "stockfish" as const,
    }));
    await db.insert(positionEvals).values(rows).onConflictDoNothing();
  }
  return { gameId: g.id };
}

export type SyntheticGame = {
  /** A fixed game id (default random), so a seed renders the same statistics every run. */
  id?: string;
  playedAt: string;
  timeClass: TimeClass;
  result: "1-0" | "0-1" | "1/2-1/2" | "*";
  playerColor?: "white" | "black";
  /** Player accuracy (null leaves it null). Default 70. */
  accuracy?: number | null;
  status?: AnalysisStatus;
  counts?: Partial<JudgementCounts>;
  opponentRating?: number;
  /** The opponent's judgements and accuracy (must never reach the player's statistics). */
  opponentCounts?: Partial<JudgementCounts>;
  /** The opponent's accuracy, written whenever the game's analysis is. Default 90. */
  opponentAccuracy?: number;
  /** A game that is not done still carries accuracy and judged moves, as a re-queued game does. */
  stale?: boolean;
  /** Player moves written (default 4, or more to fit the judgements). */
  playerMoves?: number;
  /** Write no moves rows (a game where the player made no analysed move). */
  noMoves?: boolean;
  /** The time control (default null, unknown). */
  baseSeconds?: number | null;
  incrementSeconds?: number | null;
};

type Mark = "blunder" | "mistake" | "inaccuracy";

function judgements(c: Partial<JudgementCounts> | undefined): Mark[] {
  return [
    ...Array<Mark>(c?.blunders ?? 0).fill("blunder"),
    ...Array<Mark>(c?.mistakes ?? 0).fill("mistake"),
    ...Array<Mark>(c?.inaccuracies ?? 0).fill("inaccuracy"),
  ];
}

/** A Ruy Lopez, then both knights shuffle home and back, so a game of any length stays legal. */
function syntheticSans(plies: number): string[] {
  const opening = ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6"];
  const shuffle = ["Ng1", "Ng8", "Nf3", "Nf6"];
  return Array.from({ length: plies }, (_, i) =>
    i < opening.length
      ? (opening[i] as string)
      : (shuffle[(i - opening.length) % shuffle.length] as string),
  );
}

/**
 * A short legal game whose player moves carry the given judgement counts (and the opponent's
 * moves `opponentCounts`). Accuracies and moves are written for `done` games (the default) and for
 * `stale` ones, unless `noMoves` is set. Accuracy defaults to 70 for the player, 90 for the opponent.
 */
export async function seedSyntheticGame(
  db: Db,
  accountId: string,
  row: SyntheticGame,
): Promise<{ gameId: string }> {
  await assertSeedable(db);
  const colour = row.playerColor ?? "white";
  const status = row.status ?? "done";
  const done = status === "done";
  const written = done || row.stale === true;
  const accuracy = row.accuracy === undefined ? 70 : row.accuracy;
  const opponentAccuracy = row.opponentAccuracy ?? 90;
  const wanted = judgements(row.counts);
  const theirs = judgements(row.opponentCounts);
  const sans = syntheticSans(Math.max(row.playerMoves ?? 4, wanted.length, theirs.length) * 2);
  const chess = new Chess();
  const played = sans.map((san) => {
    const m = chess.move(san);
    return { san: m.san, uci: `${m.from}${m.to}${m.promotion ?? ""}`, fenBefore: m.before };
  });
  const opponentRating = row.opponentRating ?? 1500;
  const platform = await accountPlatform(db, accountId);

  const [g] = await db
    .insert(games)
    .values({
      accountId,
      platform,
      ...(row.id ? { id: row.id } : {}),
      platformGameId: randomUUID(),
      pgn: pgnFor(
        {
          Event: "Synthetic",
          White: colour === "white" ? "me" : "opp",
          Black: colour === "white" ? "opp" : "me",
          Result: row.result,
        },
        sans,
        [],
      ),
      playedAt: new Date(row.playedAt),
      timeClass: row.timeClass,
      baseSeconds: row.baseSeconds ?? null,
      incrementSeconds: row.incrementSeconds ?? null,
      rated: true,
      playerColor: colour,
      whiteName: colour === "white" ? "me" : "opp",
      blackName: colour === "white" ? "opp" : "me",
      whiteRating: colour === "white" ? 1500 : opponentRating,
      blackRating: colour === "black" ? 1500 : opponentRating,
      result: row.result,
      analysisStatus: status,
      analysisProtocol: done ? "test-1" : null,
      whiteAccuracy: written ? (colour === "white" ? accuracy : opponentAccuracy) : null,
      blackAccuracy: written ? (colour === "black" ? accuracy : opponentAccuracy) : null,
    })
    .returning({ id: games.id });
  if (!g) throw new Error("seedSyntheticGame: no row");

  if (written && !row.noMoves) {
    const rows: MoveRow[] = played.map((m, i) => {
      const mover = i % 2 === 0 ? "white" : "black";
      return {
        gameId: g.id,
        ply: i + 1,
        color: mover,
        san: m.san,
        uci: m.uci,
        fenBefore: m.fenBefore,
        bestUci: null,
        winBefore: 50,
        winAfter: 50,
        deltaW: 0,
        accuracy: 80,
        judgement: (mover === colour ? wanted : theirs)[Math.floor(i / 2)] ?? null,
        phase: "middlegame",
        decided: false,
      };
    });
    await db.insert(moves).values(rows);
  }
  return { gameId: g.id };
}
