import { sql } from "drizzle-orm";
import {
  bigint,
  bigserial,
  boolean,
  check,
  doublePrecision,
  foreignKey,
  index,
  integer,
  interval,
  jsonb,
  pgEnum,
  pgTable,
  primaryKey,
  real,
  smallint,
  text,
  timestamp,
  unique,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";

/** Why an import run failed, from the typed platform errors. */
export type ImportErrorCode = "not_found" | "rate_limited" | "timeout" | "circuit_open" | "unknown";

/**
 * Import checkpoint on the account. Games stream newest first. `oldestReached` is the oldest
 * playedAt committed by a pass that has not finished yet (null when no pass is open), and
 * `newestSeenAt` is the playedAt watermark below which every game has been imported. ISO strings.
 */
export interface ImportCursor {
  oldestReached: string | null;
  newestSeenAt: string | null;
}

export const analysisStatus = pgEnum("analysis_status", [
  "pending",
  "analysing",
  "done",
  "failed",
  "skipped",
  "imported",
]);

export const band = pgEnum("band", ["B0", "B1", "B2", "B3", "B4", "B5"]);

export const buildStatus = pgEnum("build_status", [
  "building",
  "active",
  "superseded",
  "failed",
  "unverified",
]);

export const color = pgEnum("color", ["white", "black"]);

export const evalSource = pgEnum("eval_source", ["stockfish", "lichess", "dump"]);

export const importRunStatus = pgEnum("import_run_status", ["running", "done", "failed", "paused"]);

export const jobState = pgEnum("job_state", [
  "queued",
  "running",
  "done",
  "failed",
  "dead",
  "paused",
]);

export const judgement = pgEnum("judgement", ["inaccuracy", "mistake", "blunder"]);

export const kpiWindow = pgEnum("kpi_window", ["last_n", "week"]);

export const lineKind = pgEnum("line_kind", ["chosen", "faced"]);

export const openingVerdict = pgEnum("opening_verdict", ["keep", "fix", "learn", "replace"]);

export const phase = pgEnum("phase", ["opening", "middlegame", "endgame"]);

export const platform = pgEnum("platform", ["lichess", "chesscom"]);

export const referenceKind = pgEnum("reference_kind", ["band_reference", "opening_explorer"]);

export const result = pgEnum("result", ["1-0", "0-1", "1/2-1/2", "*"]);

export const stateRowKind = pgEnum("state_row_kind", ["transition", "onset"]);

export const timeClass = pgEnum("time_class", ["bullet", "blitz", "rapid", "classical", "daily"]);

export const weaknessState = pgEnum("weakness_state", [
  "not_an_issue",
  "emerging",
  "active",
  "improving",
  "resolved",
  "relapsed",
]);

export const bandReference = pgTable(
  "band_reference",
  {
    buildId: uuid("build_id").notNull(),
    band: band().notNull(),
    timeClass: timeClass("time_class").notNull(),
    metric: text().notNull(),
    median: real().notNull(),
    p25: real().notNull(),
    p40: real().notNull(),
    p75: real().notNull(),
    pooledRate: real("pooled_rate"),
    eplPerEvent: real("epl_per_event"),
    oppsPerGame: real("opps_per_game"),
    oppSlopePer100: real("opp_slope_per_100").notNull(),
    nPlayers: integer("n_players").notNull(),
    nGames: integer("n_games").notNull(),
    lowSample: boolean("low_sample").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "band_reference_build_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.buildId, table.band, table.timeClass, table.metric],
      name: "band_reference_pkey",
    }),
  ],
);

export const gameMetrics = pgTable(
  "game_metrics",
  {
    gameId: uuid("game_id").primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    playedAt: timestamp("played_at", { withTimezone: true, mode: "date" }).notNull(),
    protocol: text().notNull(),
    enrichmentProtocol: text("enrichment_protocol"),
    difficultyModel: text("difficulty_model"),
    metrics: jsonb().notNull(),
    metricsVersion: integer("metrics_version").generatedAlwaysAs(
      sql`COALESCE(((metrics ->> 'version'::text))::integer, 0)`,
    ),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("game_metrics_account_tc_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.playedAt.asc().nullsLast(),
    ),
    index("game_metrics_version_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.metricsVersion.asc().nullsLast(),
      table.gameId.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "game_metrics_game_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "game_metrics_account_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const gameOpenings = pgTable(
  "game_openings",
  {
    gameId: uuid("game_id").primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    color: color().notNull(),
    fenKeys: text("fen_keys").array().notNull(),
    ucis: text().array().notNull(),
    sans: text().array().notNull(),
    lineKey: text("line_key"),
    lineName: text("line_name"),
    eco: text(),
    lineKind: lineKind("line_kind"),
    lineEntryPly: integer("line_entry_ply"),
    bookExitPly: integer("book_exit_ply"),
    playerDeviationPly: integer("player_deviation_ply"),
  },
  (table) => [
    index("game_openings_account_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.color.asc().nullsLast(),
    ),
    index("game_openings_line_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.lineKey.asc().nullsLast(),
      table.lineKind.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "game_openings_account_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "game_openings_game_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const gamePreviews = pgTable(
  "game_previews",
  {
    gameId: uuid("game_id").primaryKey().notNull(),
    protocol: text().notNull(),
    whiteAccuracy: real("white_accuracy"),
    blackAccuracy: real("black_accuracy"),
    moves: jsonb().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "game_previews_game_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const games = pgTable(
  "games",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    platform: platform().notNull(),
    platformGameId: text("platform_game_id").notNull(),
    pgn: text().notNull(),
    playedAt: timestamp("played_at", { withTimezone: true, mode: "date" }).notNull(),
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
    analysisStartedAt: timestamp("analysis_started_at", { withTimezone: true, mode: "date" }),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "date" }),
    analysisAttempts: integer("analysis_attempts").default(0).notNull(),
    pendingSince: timestamp("pending_since", { withTimezone: true, mode: "date" }).defaultNow(),
    analysisGeneration: integer("analysis_generation").default(0).notNull(),
    analysedGeneration: integer("analysed_generation"),
    plyCount: integer("ply_count"),
    analysisTier: smallint("analysis_tier"),
    analysisPriority: smallint("analysis_priority"),
    selectedAt: timestamp("selected_at", { withTimezone: true, mode: "date" }),
    analysedAt: timestamp("analysed_at", { withTimezone: true, mode: "date" }),
    enrichmentProtocol: text("enrichment_protocol"),
  },
  (table) => [
    index("games_account_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.playedAt.desc().nullsLast(),
    ),
    index("games_account_status_tc_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.analysisStatus.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
    ),
    index("games_analysis_status_idx").using("btree", table.analysisStatus.asc().nullsLast()),
    index("games_attempt_cap_idx")
      .using("btree", table.analysisStatus.asc().nullsLast())
      .where(sql`(analysis_attempts >= 6)`),
    index("games_pending_id_idx")
      .using("btree", table.analysisStatus.asc().nullsLast(), table.id.asc().nullsLast())
      .where(
        sql`(analysis_status = ANY (ARRAY['pending'::analysis_status, 'analysing'::analysis_status]))`,
      ),
    index("games_started_account_idx")
      .using("btree", table.analysisStartedAt.asc().nullsLast(), table.accountId.asc().nullsLast())
      .where(sql`(analysis_started_at IS NOT NULL)`),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "games_account_id_linked_accounts_id_fk",
    }).onDelete("cascade"),
    unique("games_platform_game_uq").on(table.platform, table.platformGameId),
    check(
      "games_analysis_tier_range",
      sql`(analysis_tier IS NULL) OR ((analysis_tier >= 0) AND (analysis_tier <= 3))`,
    ),
  ],
);

export const importRuns = pgTable(
  "import_runs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    status: importRunStatus().default("running").notNull(),
    imported: integer().default(0).notNull(),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "date" }),
    lastError: text("last_error"),
    errorCode: text("error_code").$type<ImportErrorCode>(),
  },
  (table) => [
    index("import_runs_account_idx").using("btree", table.accountId.asc().nullsLast()),
    index("import_runs_paused_idx")
      .using("btree", table.accountId.asc().nullsLast())
      .where(sql`(status = 'paused'::import_run_status)`),
    index("import_runs_status_heartbeat_idx").using(
      "btree",
      table.status.asc().nullsLast(),
      table.heartbeatAt.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "import_runs_account_id_linked_accounts_id_fk",
    }).onDelete("cascade"),
  ],
);

export const jobMetrics = pgTable(
  "job_metrics",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    jobId: bigint("job_id", { mode: "number" }),
    kind: text().notNull(),
    gameId: uuid("game_id"),
    accountId: uuid("account_id"),
    queueWaitMs: doublePrecision("queue_wait_ms"),
    parseMs: doublePrecision("parse_ms"),
    cacheMs: doublePrecision("cache_ms"),
    engineWallMs: doublePrecision("engine_wall_ms"),
    engineCpuMs: doublePrecision("engine_cpu_ms"),
    persistMs: doublePrecision("persist_ms"),
    totalMs: doublePrecision("total_ms").notNull(),
    positions: integer(),
    enginePositions: integer("engine_positions"),
    cacheHits: integer("cache_hits"),
    lichessUsed: integer("lichess_used"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    stages: jsonb(),
  },
  (table) => [
    index("job_metrics_created_idx").using("btree", table.createdAt.asc().nullsLast()),
    index("job_metrics_kind_created_idx").using(
      "btree",
      table.kind.asc().nullsLast(),
      table.createdAt.asc().nullsLast(),
    ),
  ],
);

export const jobs = pgTable(
  "jobs",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    kind: text().notNull(),
    payload: jsonb().notNull(),
    dedupeKey: text("dedupe_key"),
    userKey: uuid("user_key"),
    priority: smallint().default(100).notNull(),
    runAt: timestamp("run_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    state: jobState().default("queued").notNull(),
    attempts: integer().default(0).notNull(),
    maxAttempts: integer("max_attempts").default(3).notNull(),
    stalls: integer().default(0).notNull(),
    maxStalls: integer("max_stalls").default(2).notNull(),
    recurEvery: interval("recur_every"),
    leaseOwner: text("lease_owner"),
    leaseToken: uuid("lease_token"),
    leaseUntil: timestamp("lease_until", { withTimezone: true, mode: "date" }),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "date" }),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "date" }),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "date" }),
    lastError: text("last_error"),
    errorCode: text("error_code"),
  },
  (table) => [
    uniqueIndex("jobs_dedupe_live_uq")
      .using("btree", table.kind.asc().nullsLast(), table.dedupeKey.asc().nullsLast())
      .where(
        sql`((dedupe_key IS NOT NULL) AND (state = ANY (ARRAY['queued'::job_state, 'running'::job_state, 'paused'::job_state])))`,
      ),
    index("jobs_finished_idx")
      .using("btree", table.state.asc().nullsLast(), table.finishedAt.asc().nullsLast())
      .where(sql`(state = ANY (ARRAY['done'::job_state, 'failed'::job_state, 'dead'::job_state]))`),
    index("jobs_lease_idx")
      .using("btree", table.leaseUntil.asc().nullsLast())
      .where(sql`(state = 'running'::job_state)`),
    index("jobs_ready_idx")
      .using(
        "btree",
        table.kind.asc().nullsLast(),
        table.priority.asc().nullsLast(),
        table.runAt.asc().nullsLast(),
        table.id.asc().nullsLast(),
      )
      .where(sql`(state = 'queued'::job_state)`),
    index("jobs_user_ready_idx")
      .using(
        "btree",
        table.kind.asc().nullsLast(),
        table.userKey.asc().nullsLast(),
        table.priority.asc().nullsLast(),
        table.runAt.asc().nullsLast(),
      )
      .where(sql`(state = 'queued'::job_state)`),
  ],
);

export const kpiSnapshot = pgTable(
  "kpi_snapshot",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    userId: uuid("user_id"),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    metric: text().notNull(),
    windowKind: kpiWindow("window_kind").notNull(),
    windowKey: text("window_key").notNull(),
    value: doublePrecision(),
    valueAdjusted: doublePrecision("value_adjusted"),
    n: integer().notNull(),
    ciLow: doublePrecision("ci_low"),
    ciHigh: doublePrecision("ci_high"),
    protocol: text().notNull(),
    computedAt: timestamp("computed_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("kpi_snapshot_lookup_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.metric.asc().nullsLast(),
      table.windowKey.asc().nullsLast(),
      table.computedAt.desc().nullsFirst(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "kpi_snapshot_account_id_fkey",
    }).onDelete("cascade"),
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
    verifiedAt: timestamp("verified_at", { withTimezone: true, mode: "date" }),
    lastSyncedAt: timestamp("last_synced_at", { withTimezone: true, mode: "date" }),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    importCursor: jsonb("import_cursor").$type<ImportCursor>(),
    analysisScheduledAt: timestamp("analysis_scheduled_at", { withTimezone: true, mode: "date" }),
  },
  (table) => [unique("linked_accounts_platform_username_uq").on(table.platform, table.username)],
);

export const moveFeatures = pgTable(
  "move_features",
  {
    gameId: uuid("game_id").notNull(),
    ply: integer().notNull(),
    multipv: jsonb(),
    bestPv: text("best_pv").array().default(sql`'{}'::text[]`).notNull(),
    refutationPv: text("refutation_pv").array().default(sql`'{}'::text[]`).notNull(),
    threat: jsonb(),
    tbWdlBefore: smallint("tb_wdl_before"),
    tbWdlAfter: smallint("tb_wdl_after"),
    pBest: real("p_best"),
    pOk: real("p_ok"),
    difficultyModel: text("difficulty_model"),
    isCritical: boolean("is_critical"),
    multipvGap: real("multipv_gap"),
    inTop2: boolean("in_top2"),
    bestLine: jsonb("best_line"),
    refutationLine: jsonb("refutation_line"),
    threatLine: jsonb("threat_line"),
    threatExisted: boolean("threat_existed"),
    threatAddressed: boolean("threat_addressed"),
    enrichmentProtocol: text("enrichment_protocol").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "move_features_game_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.gameId, table.ply], name: "move_features_pkey" }),
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

export const openingExplorer = pgTable(
  "opening_explorer",
  {
    buildId: uuid("build_id").notNull(),
    fenKey: text("fen_key").notNull(),
    band: band().notNull(),
    timeClass: timeClass("time_class").notNull(),
    uci: text().notNull(),
    white: integer().notNull(),
    draws: integer().notNull(),
    black: integer().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "opening_explorer_build_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.buildId, table.fenKey, table.band, table.timeClass, table.uci],
      name: "opening_explorer_pkey",
    }),
  ],
);

export const openingLines = pgTable(
  "opening_lines",
  {
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    color: color().notNull(),
    lineKey: text("line_key").notNull(),
    kind: lineKind().notNull(),
    games: integer().notNull(),
    verdict: openingVerdict(),
    summary: jsonb().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "opening_lines_account_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.accountId, table.timeClass, table.color, table.lineKey, table.kind],
      name: "opening_lines_pkey",
    }),
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
    pv: text().array().default(sql`'{}'::text[]`).notNull(),
    source: evalSource().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    primaryKey({
      columns: [table.fenKey, table.protocol],
      name: "position_evals_fen_key_protocol_pk",
    }),
  ],
);

export const positionMultipv = pgTable(
  "position_multipv",
  {
    fenKey: text("fen_key").notNull(),
    protocol: text().notNull(),
    lines: jsonb().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.fenKey, table.protocol], name: "position_multipv_pkey" }),
  ],
);

export const positionTb = pgTable("position_tb", {
  fenKey: text("fen_key").primaryKey().notNull(),
  wdl: smallint().notNull(),
  source: text().default("syzygy5").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
});

export const positionThreats = pgTable(
  "position_threats",
  {
    fenKey: text("fen_key").notNull(),
    protocol: text().notNull(),
    scoreCp: integer("score_cp"),
    scoreMate: integer("score_mate"),
    pv: text().array().default(sql`'{}'::text[]`).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    primaryKey({ columns: [table.fenKey, table.protocol], name: "position_threats_pkey" }),
  ],
);

export const referenceBuilds = pgTable(
  "reference_builds",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    kind: referenceKind().notNull(),
    status: buildStatus().default("building").notNull(),
    sourceMonth: text("source_month").notNull(),
    sourceSha256: text("source_sha256").notNull(),
    sourceUrl: text("source_url"),
    params: jsonb().default({}).notNull(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    gamesScanned: bigint("games_scanned", { mode: "number" }).default(0).notNull(),
    protocol: text(),
    enrichmentProtocol: text("enrichment_protocol"),
    difficultyModel: text("difficulty_model"),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "date" }),
    activatedAt: timestamp("activated_at", { withTimezone: true, mode: "date" }),
    error: text(),
  },
  (table) => [
    uniqueIndex("reference_builds_one_active_uq")
      .using("btree", table.kind.asc().nullsLast())
      .where(sql`(status = 'active'::build_status)`),
  ],
);

export const referenceGames = pgTable(
  "reference_games",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    buildId: uuid("build_id").notNull(),
    lichessGameId: text("lichess_game_id").notNull(),
    pgn: text().notNull(),
    timeClass: timeClass("time_class").notNull(),
    baseSeconds: integer("base_seconds"),
    incrementSeconds: integer("increment_seconds"),
    whiteRating: integer("white_rating"),
    blackRating: integer("black_rating"),
    whiteHash: text("white_hash").notNull(),
    blackHash: text("black_hash").notNull(),
    result: result().notNull(),
    termination: text(),
    playedAt: timestamp("played_at", { withTimezone: true, mode: "date" }).notNull(),
    status: analysisStatus().default("pending").notNull(),
    error: text(),
  },
  (table) => [
    index("reference_games_build_status_idx").using(
      "btree",
      table.buildId.asc().nullsLast(),
      table.status.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "reference_games_build_id_fkey",
    }).onDelete("cascade"),
    unique("reference_games_build_game_uq").on(table.buildId, table.lichessGameId),
  ],
);

export const referenceObservations = pgTable(
  "reference_observations",
  {
    buildId: uuid("build_id").notNull(),
    referenceGameId: uuid("reference_game_id").notNull(),
    color: color().notNull(),
    playerHash: text("player_hash").notNull(),
    band: band().notNull(),
    timeClass: timeClass("time_class").notNull(),
    playerRating: integer("player_rating").notNull(),
    opponentRating: integer("opponent_rating").notNull(),
    metrics: jsonb().notNull(),
  },
  (table) => [
    index("reference_observations_cell_idx").using(
      "btree",
      table.buildId.asc().nullsLast(),
      table.band.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "reference_observations_build_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.referenceGameId],
      foreignColumns: [referenceGames.id],
      name: "reference_observations_reference_game_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.referenceGameId, table.color],
      name: "reference_observations_pkey",
    }),
  ],
);

export const repertoireNodes = pgTable(
  "repertoire_nodes",
  {
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    color: color().notNull(),
    fenKey: text("fen_key").notNull(),
    games: integer().notNull(),
    stats: jsonb().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "repertoire_nodes_account_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.accountId, table.timeClass, table.color, table.fenKey],
      name: "repertoire_nodes_pkey",
    }),
  ],
);

export const userEvents = pgTable(
  "user_events",
  {
    id: bigserial({ mode: "number" }).primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    type: text().notNull(),
    payload: jsonb().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("user_events_account_id_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.id.asc().nullsLast(),
    ),
    index("user_events_created_idx").using("btree", table.createdAt.asc().nullsLast()),
  ],
);

export const userEventsWatermark = pgTable(
  "user_events_watermark",
  {
    singleton: boolean().default(true).primaryKey().notNull(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    prunedThroughId: bigint("pruned_through_id", { mode: "number" }).default(0).notNull(),
  },
  (_table) => [check("user_events_watermark_singleton_check", sql`CHECK (singleton)`)],
);

export const weaknessEstimates = pgTable(
  "weakness_estimates",
  {
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    seriesKey: text("series_key").notNull(),
    k: doublePrecision().notNull(),
    n: doublePrecision().notNull(),
    nUnweighted: integer("n_unweighted").notNull(),
    kUnweighted: doublePrecision("k_unweighted").notNull(),
    alpha: doublePrecision().notNull(),
    beta: doublePrecision().notNull(),
    mean: doublePrecision().notNull(),
    ciLow: doublePrecision("ci_low").notNull(),
    ciHigh: doublePrecision("ci_high").notNull(),
    bandMedian: doublePrecision("band_median"),
    bandP40: doublePrecision("band_p40"),
    confidence: doublePrecision().notNull(),
    excess: doublePrecision(),
    priority: doublePrecision().notNull(),
    percentile: doublePrecision(),
    state: weaknessState().notNull(),
    pImp: doublePrecision("p_imp"),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weakness_estimates_account_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.accountId, table.timeClass, table.seriesKey],
      name: "weakness_estimates_pkey",
    }),
  ],
);

export const weaknessSeriesPoint = pgTable(
  "weakness_series_point",
  {
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    seriesKey: text("series_key").notNull(),
    gameId: uuid("game_id").notNull(),
    playedAt: timestamp("played_at", { withTimezone: true, mode: "date" }).notNull(),
    events: integer().notNull(),
    opportunities: integer().notNull(),
    epl: real().notNull(),
    protocol: text().notNull(),
  },
  (table) => [
    index("weakness_series_point_game_idx").using("btree", table.gameId.asc().nullsLast()),
    index("weakness_series_point_series_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.seriesKey.asc().nullsLast(),
      table.playedAt.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weakness_series_point_account_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "weakness_series_point_game_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.accountId, table.timeClass, table.seriesKey, table.gameId],
      name: "weakness_series_point_pkey",
    }),
  ],
);

export const weaknessStateHistory = pgTable(
  "weakness_state_history",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    seriesKey: text("series_key").notNull(),
    kind: stateRowKind().notNull(),
    fromState: weaknessState("from_state"),
    toState: weaknessState("to_state").notNull(),
    atGameId: uuid("at_game_id"),
    atPlayedAt: timestamp("at_played_at", { withTimezone: true, mode: "date" }),
    onsetGameId: uuid("onset_game_id"),
    onsetAt: timestamp("onset_at", { withTimezone: true, mode: "date" }),
    onsetLow: timestamp("onset_low", { withTimezone: true, mode: "date" }),
    onsetHigh: timestamp("onset_high", { withTimezone: true, mode: "date" }),
    pChange: real("p_change"),
    rateBefore: real("rate_before"),
    rateAfter: real("rate_after"),
    evidence: jsonb().notNull(),
    causeHints: jsonb("cause_hints").default([]).notNull(),
    detectorVersion: text("detector_version").notNull(),
    protocol: text().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("weakness_state_history_series_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.seriesKey.asc().nullsLast(),
      table.id.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weakness_state_history_account_id_fkey",
    }).onDelete("cascade"),
  ],
);
