import { sql } from "drizzle-orm";
import {
  boolean,
  doublePrecision,
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  primaryKey,
  real,
  text,
  timestamp,
  unique,
  uuid,
} from "drizzle-orm/pg-core";

export const platformEnum = pgEnum("platform", ["lichess", "chesscom"]);
export const timeClassEnum = pgEnum("time_class", [
  "bullet",
  "blitz",
  "rapid",
  "classical",
  "daily",
]);
export const colorEnum = pgEnum("color", ["white", "black"]);
export const resultEnum = pgEnum("result", ["1-0", "0-1", "1/2-1/2", "*"]);
export const analysisStatusEnum = pgEnum("analysis_status", [
  "pending",
  "analysing",
  "done",
  "failed",
  "skipped",
]);
export const evalSourceEnum = pgEnum("eval_source", ["stockfish", "lichess", "dump"]);
export const judgementEnum = pgEnum("judgement", ["inaccuracy", "mistake", "blunder"]);
export const phaseEnum = pgEnum("phase", ["opening", "middlegame", "endgame"]);

export const linkedAccounts = pgTable(
  "linked_accounts",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id"),
    platform: platformEnum("platform").notNull(),
    /** Always stored lower-cased. */
    username: text("username").notNull(),
    displayName: text("display_name").notNull(),
    verifiedAt: timestamp("verified_at", { withTimezone: true }),
    lastSyncedAt: timestamp("last_synced_at", { withTimezone: true }),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [unique("linked_accounts_platform_username_uq").on(t.platform, t.username)],
);

export const games = pgTable(
  "games",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    accountId: uuid("account_id")
      .notNull()
      .references(() => linkedAccounts.id),
    platform: platformEnum("platform").notNull(),
    platformGameId: text("platform_game_id").notNull(),
    pgn: text("pgn").notNull(),
    playedAt: timestamp("played_at", { withTimezone: true }).notNull(),
    timeClass: timeClassEnum("time_class").notNull(),
    baseSeconds: integer("base_seconds"),
    incrementSeconds: integer("increment_seconds"),
    rated: boolean("rated").notNull(),
    playerColor: colorEnum("player_color").notNull(),
    whiteName: text("white_name").notNull(),
    blackName: text("black_name").notNull(),
    whiteRating: integer("white_rating"),
    blackRating: integer("black_rating"),
    result: resultEnum("result").notNull(),
    termination: text("termination"),
    eco: text("eco"),
    openingName: text("opening_name"),
    lichessEvals: jsonb("lichess_evals"),
    analysisStatus: analysisStatusEnum("analysis_status").notNull().default("pending"),
    analysisProtocol: text("analysis_protocol"),
    analysisError: text("analysis_error"),
    whiteAccuracy: real("white_accuracy"),
    blackAccuracy: real("black_accuracy"),
  },
  (t) => [
    unique("games_platform_game_uq").on(t.platform, t.platformGameId),
    index("games_account_played_idx").on(t.accountId, t.playedAt.desc()),
  ],
);

export const positionEvals = pgTable(
  "position_evals",
  {
    fenKey: text("fen_key").notNull(),
    protocol: text("protocol").notNull(),
    /** White point of view. */
    scoreCp: integer("score_cp"),
    scoreMate: integer("score_mate"),
    bestUci: text("best_uci"),
    pv: text("pv").array().notNull().default(sql`'{}'::text[]`),
    source: evalSourceEnum("source").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.fenKey, t.protocol] })],
);

export const moves = pgTable(
  "moves",
  {
    gameId: uuid("game_id")
      .notNull()
      .references(() => games.id, { onDelete: "cascade" }),
    ply: integer("ply").notNull(),
    color: colorEnum("color").notNull(),
    san: text("san").notNull(),
    uci: text("uci").notNull(),
    fenBefore: text("fen_before").notNull(),
    bestUci: text("best_uci"),
    winBefore: doublePrecision("win_before").notNull(),
    winAfter: doublePrecision("win_after").notNull(),
    deltaW: doublePrecision("delta_w").notNull(),
    accuracy: doublePrecision("accuracy").notNull(),
    judgement: judgementEnum("judgement"),
    phase: phaseEnum("phase").notNull(),
    clockBefore: doublePrecision("clock_before"),
    timeSpent: doublePrecision("time_spent"),
    decided: boolean("decided").notNull(),
  },
  (t) => [primaryKey({ columns: [t.gameId, t.ply] })],
);
