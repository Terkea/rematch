import {
  bigserial,
  boolean,
  doublePrecision,
  foreignKey,
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

export const analysisStatus = pgEnum("analysis_status", [
  "pending",
  "analysing",
  "done",
  "failed",
  "skipped",
]);
export const color = pgEnum("color", ["white", "black"]);
export const evalSource = pgEnum("eval_source", ["stockfish", "lichess", "dump"]);
export const importRunStatus = pgEnum("import_run_status", ["running", "done", "failed", "paused"]);
export const judgement = pgEnum("judgement", ["inaccuracy", "mistake", "blunder"]);
export const phase = pgEnum("phase", ["opening", "middlegame", "endgame"]);
export const platform = pgEnum("platform", ["lichess", "chesscom"]);
export const result = pgEnum("result", ["1-0", "0-1", "1/2-1/2", "*"]);
export const timeClass = pgEnum("time_class", ["bullet", "blitz", "rapid", "classical", "daily"]);

export const userEvents = pgTable(
  "user_events",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    type: text().notNull(),
    payload: jsonb().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("user_events_account_id_idx").using(
      "btree",
      table.accountId.asc().nullsLast().op("int8_ops"),
      table.id.asc().nullsLast().op("int8_ops"),
    ),
    index("user_events_created_idx").using(
      "btree",
      table.createdAt.asc().nullsLast().op("timestamptz_ops"),
    ),
  ],
);

export const linkedAccounts = pgTable(
  "linked_accounts",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id"),
    platform: platform().notNull(),
    username: text().notNull(),
    displayName: text("display_name").notNull(),
    verifiedAt: timestamp("verified_at", { withTimezone: true, mode: "string" }),
    lastSyncedAt: timestamp("last_synced_at", { withTimezone: true, mode: "string" }),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    importCursor: jsonb("import_cursor"),
  },
  (table) => [unique("linked_accounts_platform_username_uq").on(table.platform, table.username)],
);

export const games = pgTable(
  "games",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    platform: platform().notNull(),
    platformGameId: text("platform_game_id").notNull(),
    pgn: text().notNull(),
    playedAt: timestamp("played_at", { withTimezone: true, mode: "string" }).notNull(),
    timeClass: timeClass("time_class").notNull(),
    baseSeconds: integer("base_seconds"),
    incrementSeconds: integer("increment_seconds"),
    rated: boolean().notNull(),
    playerColor: color("player_color").notNull(),
    whiteName: text("white_name").notNull(),
    blackName: text("black_name").notNull(),
    whiteRating: integer("white_rating"),
    blackRating: integer("black_rating"),
    result: result().notNull(),
    termination: text(),
    eco: text(),
    openingName: text("opening_name"),
    lichessEvals: jsonb("lichess_evals"),
    analysisStatus: analysisStatus("analysis_status").default("pending").notNull(),
    analysisProtocol: text("analysis_protocol"),
    analysisError: text("analysis_error"),
    whiteAccuracy: real("white_accuracy"),
    blackAccuracy: real("black_accuracy"),
    analysisStartedAt: timestamp("analysis_started_at", { withTimezone: true, mode: "string" }),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "string" }),
    analysisAttempts: integer("analysis_attempts").default(0).notNull(),
    pendingSince: timestamp("pending_since", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("games_account_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast().op("timestamptz_ops"),
      table.playedAt.desc().nullsLast().op("uuid_ops"),
    ),
    index("games_analysis_status_idx").using(
      "btree",
      table.analysisStatus.asc().nullsLast().op("enum_ops"),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "games_account_id_linked_accounts_id_fk",
    }),
    unique("games_platform_game_uq").on(table.platform, table.platformGameId),
  ],
);

export const importRuns = pgTable(
  "import_runs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    status: importRunStatus().default("running").notNull(),
    imported: integer().default(0).notNull(),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "string" }),
    lastError: text("last_error"),
    errorCode: text("error_code"),
  },
  (table) => [
    index("import_runs_account_idx").using(
      "btree",
      table.accountId.asc().nullsLast().op("uuid_ops"),
    ),
    index("import_runs_status_heartbeat_idx").using(
      "btree",
      table.status.asc().nullsLast().op("timestamptz_ops"),
      table.heartbeatAt.asc().nullsLast().op("timestamptz_ops"),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "import_runs_account_id_linked_accounts_id_fk",
    }).onDelete("cascade"),
  ],
);

export const positionEvals = pgTable(
  "position_evals",
  {
    fenKey: text("fen_key").notNull(),
    protocol: text().notNull(),
    scoreCp: integer("score_cp"),
    scoreMate: integer("score_mate"),
    bestUci: text("best_uci"),
    pv: text().array().default([""]).notNull(),
    source: evalSource().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    primaryKey({
      columns: [table.fenKey, table.protocol],
      name: "position_evals_fen_key_protocol_pk",
    }),
  ],
);

export const moves = pgTable(
  "moves",
  {
    gameId: uuid("game_id").notNull(),
    ply: integer().notNull(),
    color: color().notNull(),
    san: text().notNull(),
    uci: text().notNull(),
    fenBefore: text("fen_before").notNull(),
    bestUci: text("best_uci"),
    winBefore: doublePrecision("win_before").notNull(),
    winAfter: doublePrecision("win_after").notNull(),
    deltaW: doublePrecision("delta_w").notNull(),
    accuracy: doublePrecision().notNull(),
    judgement: judgement(),
    phase: phase().notNull(),
    clockBefore: doublePrecision("clock_before"),
    timeSpent: doublePrecision("time_spent"),
    decided: boolean().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "moves_game_id_games_id_fk",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.gameId, table.ply], name: "moves_game_id_ply_pk" }),
  ],
);
