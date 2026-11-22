import { sql } from "drizzle-orm";
import {
  bigint,
  bigserial,
  boolean,
  check,
  date,
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

export const aiBatchStatus = pgEnum("ai_batch_status", [
  "submitted",
  "ended",
  "processed",
  "failed",
]);

export const aiFeature = pgEnum("ai_feature", [
  "report",
  "weekly_review",
  "weakness_change",
  "chat",
]);

export const aiItemStatus = pgEnum("ai_item_status", ["pending", "ready", "dropped", "failed"]);

export const aiPlan = pgEnum("ai_plan", ["free", "pro", "beta"]);

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

export const chatRole = pgEnum("chat_role", ["user", "assistant"]);

export const chatStatus = pgEnum("chat_status", ["pending", "streaming", "done", "failed"]);

export const color = pgEnum("color", ["white", "black"]);

export const evalSource = pgEnum("eval_source", ["stockfish", "lichess", "dump"]);

export const importRunStatus = pgEnum("import_run_status", ["running", "done", "failed", "paused"]);

export const interactiveEvalStatus = pgEnum("interactive_eval_status", [
  "pending",
  "done",
  "failed",
]);

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

export const linkMethod = pgEnum("link_method", [
  "username",
  "location_code",
  "lichess_oauth",
  "chesscom_oauth",
]);

export const openingVerdict = pgEnum("opening_verdict", ["keep", "fix", "learn", "replace"]);

export const phase = pgEnum("phase", ["opening", "middlegame", "endgame"]);

export const platform = pgEnum("platform", ["lichess", "chesscom"]);

export const referenceKind = pgEnum("reference_kind", ["band_reference", "opening_explorer"]);

export const reportMode = pgEnum("report_mode", ["sync", "batch"]);

export const reportRunStatus = pgEnum("report_run_status", [
  "queued",
  "running",
  "awaiting_retry",
  "awaiting_batch",
  "done",
  "failed",
]);

export const reportStatus = pgEnum("report_status", ["computed_only", "partial", "complete"]);

export const reportTrigger = pgEnum("report_trigger", [
  "onboarding",
  "manual",
  "scheduled",
  "profile_change",
]);

export const result = pgEnum("result", ["1-0", "0-1", "1/2-1/2", "*"]);

export const sideScope = pgEnum("side_scope", ["both", "white", "black"]);

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

export const account = pgTable(
  "account",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: text("account_id").notNull(),
    providerId: text("provider_id").notNull(),
    userId: uuid("user_id").notNull(),
    accessToken: text("access_token"),
    refreshToken: text("refresh_token"),
    idToken: text("id_token"),
    accessTokenExpiresAt: timestamp("access_token_expires_at", {
      withTimezone: true,
      mode: "date",
    }),
    refreshTokenExpiresAt: timestamp("refresh_token_expires_at", {
      withTimezone: true,
      mode: "date",
    }),
    scope: text(),
    password: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("account_user_id_idx").using("btree", table.userId.asc().nullsLast()),
    foreignKey({
      columns: [table.userId],
      foreignColumns: [user.id],
      name: "account_user_id_user_id_fk",
    }).onDelete("cascade"),
    unique("account_provider_account_unique").on(table.accountId, table.providerId),
  ],
);

export const accountLinks = pgTable(
  "account_links",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    linkedAccountId: uuid("linked_account_id").notNull(),
    method: linkMethod().notNull(),
    verifiedAt: timestamp("verified_at", { withTimezone: true, mode: "date" }),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    uniqueIndex("account_links_one_verified_owner")
      .using("btree", table.linkedAccountId.asc().nullsLast())
      .where(sql`(verified_at IS NOT NULL)`),
    foreignKey({
      columns: [table.userId],
      foreignColumns: [user.id],
      name: "account_links_user_id_user_id_fk",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.linkedAccountId],
      foreignColumns: [linkedAccounts.id],
      name: "account_links_linked_account_id_linked_accounts_id_fk",
    }).onDelete("cascade"),
    unique("account_links_user_account_unique").on(table.userId, table.linkedAccountId),
  ],
);

export const aiBatchItems = pgTable(
  "ai_batch_items",
  {
    id: uuid().primaryKey().notNull(),
    batchId: text("batch_id").notNull(),
    customId: text("custom_id").notNull(),
    kind: text().notNull(),
    reportRunId: uuid("report_run_id"),
    section: text(),
    weeklyReviewId: uuid("weekly_review_id"),
    processedAt: timestamp("processed_at", { withTimezone: true, mode: "date" }),
  },
  (table) => [
    index("ai_batch_items_pending_idx")
      .using("btree", table.batchId.asc().nullsLast())
      .where(sql`(processed_at IS NULL)`),
    index("ai_batch_items_run_idx")
      .using("btree", table.reportRunId.asc().nullsLast())
      .where(sql`(report_run_id IS NOT NULL)`),
    index("ai_batch_items_weekly_idx")
      .using("btree", table.weeklyReviewId.asc().nullsLast())
      .where(sql`(weekly_review_id IS NOT NULL)`),
    foreignKey({
      columns: [table.batchId],
      foreignColumns: [aiBatches.id],
      name: "ai_batch_items_batch_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.reportRunId],
      foreignColumns: [reportRuns.id],
      name: "ai_batch_items_report_run_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.weeklyReviewId],
      foreignColumns: [weeklyReviews.id],
      name: "ai_batch_items_weekly_review_id_fkey",
    }).onDelete("cascade"),
    unique("ai_batch_items_custom_id_key").on(table.customId),
    check(
      "ai_batch_items_kind_check",
      sql`kind = ANY (ARRAY['report'::text, 'weekly_review'::text])`,
    ),
  ],
);

export const aiBatches = pgTable(
  "ai_batches",
  {
    id: text().primaryKey().notNull(),
    kind: text().notNull(),
    status: aiBatchStatus().default("submitted").notNull(),
    requestCount: integer("request_count").notNull(),
    submittedAt: timestamp("submitted_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
    endedAt: timestamp("ended_at", { withTimezone: true, mode: "date" }),
    processedAt: timestamp("processed_at", { withTimezone: true, mode: "date" }),
    lastPolledAt: timestamp("last_polled_at", { withTimezone: true, mode: "date" }),
  },
  (table) => [
    index("ai_batches_open_idx")
      .using("btree", table.lastPolledAt.asc().nullsFirst(), table.submittedAt.asc().nullsLast())
      .where(sql`(status = ANY (ARRAY['submitted'::ai_batch_status, 'ended'::ai_batch_status]))`),
    check("ai_batches_kind_check", sql`kind = ANY (ARRAY['report'::text, 'weekly_review'::text])`),
  ],
);

export const aiSdkSessionEntries = pgTable(
  "ai_sdk_session_entries",
  {
    projectKey: text("project_key").notNull(),
    sessionId: uuid("session_id").notNull(),
    subpath: text().default("").notNull(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    seq: bigint({ mode: "number" }).notNull(),
    entry: jsonb().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("ai_sdk_session_entries_created_idx").using("btree", table.createdAt.asc().nullsLast()),
    index("ai_sdk_session_entries_session_idx").using("btree", table.sessionId.asc().nullsLast()),
    uniqueIndex("ai_sdk_session_entries_uuid_uq")
      .using(
        "btree",
        sql`project_key`,
        sql`session_id`,
        sql`subpath`,
        sql`((entry ->> 'uuid'::text))`,
      )
      .where(sql`(entry ? 'uuid'::text)`),
    primaryKey({
      columns: [table.projectKey, table.sessionId, table.subpath, table.seq],
      name: "ai_sdk_session_entries_pkey",
    }),
  ],
);

export const aiStatus = pgTable(
  "ai_status",
  {
    workerId: uuid("worker_id").primaryKey().notNull(),
    enabled: boolean().notNull(),
    detail: text().notNull(),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
  },
  (table) => [index("ai_status_heartbeat_idx").using("btree", table.heartbeatAt.asc().nullsLast())],
);

export const aiSweepCursors = pgTable("ai_sweep_cursors", {
  name: text().primaryKey().notNull(),
  position: timestamp({ withTimezone: true, mode: "date" }).notNull(),
  updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
});

export const aiUsage = pgTable(
  "ai_usage",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    feature: aiFeature().notNull(),
    model: text().notNull(),
    batch: boolean().default(false).notNull(),
    inputTokens: integer("input_tokens").notNull(),
    outputTokens: integer("output_tokens").notNull(),
    cacheCreationTokens: integer("cache_creation_tokens").notNull(),
    cacheReadTokens: integer("cache_read_tokens").notNull(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    costMicroUsd: bigint("cost_micro_usd", { mode: "number" }).notNull(),
    refId: text("ref_id"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    idempotencyKey: text("idempotency_key"),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    sdkEstimateMicroUsd: bigint("sdk_estimate_micro_usd", { mode: "number" }),
  },
  (table) => [
    index("ai_usage_account_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.createdAt.asc().nullsLast(),
    ),
    index("ai_usage_created_idx").using("btree", table.createdAt.asc().nullsLast()),
    uniqueIndex("ai_usage_idempotency_uq")
      .using("btree", table.idempotencyKey.asc().nullsLast())
      .where(sql`(idempotency_key IS NOT NULL)`),
  ],
);

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
    oppSlopePer100: real("opp_slope_per_100"),
    nPlayers: integer("n_players").notNull(),
    nGames: integer("n_games").notNull(),
    lowSample: boolean("low_sample").notNull(),
    side: sideScope().default("both").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "band_reference_build_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.buildId, table.band, table.timeClass, table.metric, table.side],
      name: "band_reference_pkey",
    }),
    check(
      "band_reference_slope_finite",
      sql`(opp_slope_per_100 IS NULL) OR (opp_slope_per_100 <> ALL (ARRAY['NaN'::real, 'Infinity'::real, '-Infinity'::real]))`,
    ),
  ],
);

export const chesscomProfileRequests = pgTable("chesscom_profile_requests", {
  username: text().primaryKey().notNull(),
  requestedAt: timestamp("requested_at", { withTimezone: true, mode: "date" }).notNull(),
});

export const chesscomRequestBudgets = pgTable(
  "chesscom_request_budgets",
  {
    userId: uuid("user_id").primaryKey().notNull(),
    tokens: doublePrecision().notNull(),
    refilledAt: timestamp("refilled_at", { withTimezone: true, mode: "date" }).notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.userId],
      foreignColumns: [user.id],
      name: "chesscom_request_budgets_user_id_user_id_fk",
    }).onDelete("cascade"),
  ],
);

export const chesscomVerifications = pgTable(
  "chesscom_verifications",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountLinkId: uuid("account_link_id").notNull(),
    code: text().notNull(),
    expiresAt: timestamp("expires_at", { withTimezone: true, mode: "date" }).notNull(),
    lastCheckedAt: timestamp("last_checked_at", { withTimezone: true, mode: "date" }),
    checks: integer().default(0).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountLinkId],
      foreignColumns: [accountLinks.id],
      name: "chesscom_verifications_account_link_id_account_links_id_fk",
    }).onDelete("cascade"),
    unique("chesscom_verifications_account_link_id_unique").on(table.accountLinkId),
    check(
      "chesscom_verifications_code_format",
      sql`code ~ '^CI-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{6}$'::text`,
    ),
  ],
);

export const coachConversations = pgTable(
  "coach_conversations",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    userId: uuid("user_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    title: text(),
    anchor: jsonb(),
    summary: text(),
    summaryThroughSeq: integer("summary_through_seq").default(0).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    lastMessageAt: timestamp("last_message_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
    nextCallIndex: integer("next_call_index").default(0).notNull(),
  },
  (table) => [
    index("coach_conversations_owner_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.lastMessageAt.desc().nullsFirst(),
    ),
    index("coach_conversations_user_idx").using(
      "btree",
      table.userId.asc().nullsLast(),
      table.accountId.asc().nullsLast(),
      table.lastMessageAt.desc().nullsFirst(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "coach_conversations_account_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.userId],
      foreignColumns: [user.id],
      name: "coach_conversations_user_id_fk",
    }).onDelete("cascade"),
    check("coach_conversations_next_call_index_check", sql`next_call_index >= 0`),
  ],
);

export const coachMessages = pgTable(
  "coach_messages",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    conversationId: uuid("conversation_id").notNull(),
    seq: integer().notNull(),
    role: chatRole().notNull(),
    status: chatStatus().notNull(),
    anchor: jsonb(),
    text: text(),
    segments: jsonb(),
    citations: jsonb().default([]).notNull(),
    apiMessages: jsonb("api_messages"),
    droppedCount: integer("dropped_count").default(0).notNull(),
    failureReason: text("failure_reason"),
    aiDisclosure: jsonb("ai_disclosure"),
    session: text(),
    sessionModelUsage: jsonb("session_model_usage"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    streamingAt: timestamp("streaming_at", { withTimezone: true, mode: "date" }),
    sessionInSync: boolean("session_in_sync"),
    turnAttempts: smallint("turn_attempts").default(0).notNull(),
  },
  (table) => [
    index("coach_messages_pending_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(sql`(status = 'pending'::chat_status)`),
    index("coach_messages_streaming_idx")
      .using("btree", table.streamingAt.asc().nullsLast())
      .where(sql`(status = 'streaming'::chat_status)`),
    foreignKey({
      columns: [table.conversationId],
      foreignColumns: [coachConversations.id],
      name: "coach_messages_conversation_id_fkey",
    }).onDelete("cascade"),
    unique("coach_messages_conversation_id_seq_key").on(table.conversationId, table.seq),
    check(
      "coach_messages_session_check",
      sql`session = ANY (ARRAY['new'::text, 'resumed'::text, 'rebuilt'::text])`,
    ),
    check("coach_messages_turn_attempts_check", sql`turn_attempts >= 0`),
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
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "game_openings_game_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "game_openings_account_id_fkey",
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
    profileStaleAt: timestamp("profile_stale_at", { withTimezone: true, mode: "date" }),
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
    index("games_imported_waiting_idx")
      .using("btree", table.accountId.asc().nullsLast())
      .where(
        sql`((analysis_status = 'imported'::analysis_status) AND (pending_since IS NOT NULL))`,
      ),
    index("games_pending_id_idx")
      .using("btree", table.analysisStatus.asc().nullsLast(), table.id.asc().nullsLast())
      .where(
        sql`(analysis_status = ANY (ARRAY['pending'::analysis_status, 'analysing'::analysis_status]))`,
      ),
    index("games_profile_stale_idx")
      .using("btree", table.profileStaleAt.asc().nullsLast(), table.id.asc().nullsLast())
      .where(sql`(profile_stale_at IS NOT NULL)`),
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

export const interactiveEvals = pgTable(
  "interactive_evals",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    messageId: uuid("message_id"),
    source: text().default("interactive").notNull(),
    protocol: text().notNull(),
    baseFen: text("base_fen").notNull(),
    moves: text().array().default(sql`'{}'::text[]`).notNull(),
    fen: text().notNull(),
    status: interactiveEvalStatus().default("pending").notNull(),
    lines: jsonb(),
    error: text(),
    charged: boolean().default(true).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "date" }),
  },
  (table) => [
    index("interactive_evals_message_idx")
      .using("btree", table.messageId.asc().nullsLast())
      .where(sql`(message_id IS NOT NULL)`),
    index("interactive_evals_pending_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(sql`(status = 'pending'::interactive_eval_status)`),
    index("interactive_evals_quota_idx")
      .using("btree", table.accountId.asc().nullsLast(), table.createdAt.asc().nullsLast())
      .where(sql`charged`),
    index("interactive_evals_reuse_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.fen.asc().nullsLast(),
      table.protocol.asc().nullsLast(),
      table.createdAt.desc().nullsFirst(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "interactive_evals_account_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.messageId],
      foreignColumns: [coachMessages.id],
      name: "interactive_evals_message_id_fkey",
    }).onDelete("set null"),
    check("interactive_evals_source_check", sql`source = 'interactive'::text`),
  ],
);

export const invites = pgTable(
  "invites",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    email: text().notNull(),
    note: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    unique("invites_email_unique").on(table.email),
    check("invites_email_lower", sql`email = lower(email)`),
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
        table.id.asc().nullsLast(),
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
    side: sideScope().default("both").notNull(),
  },
  (table) => [
    index("kpi_snapshot_lookup_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.side.asc().nullsLast(),
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

export const magicLinkSends = pgTable(
  "magic_link_sends",
  {
    id: bigserial({ mode: "bigint" }).primaryKey().notNull(),
    email: text().notNull(),
    sentAt: timestamp("sent_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("magic_link_sends_email_sent_at_idx").using(
      "btree",
      table.email.asc().nullsLast(),
      table.sentAt.asc().nullsLast(),
    ),
  ],
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
    difficultyStatus: text("difficulty_status"),
    difficultyReason: text("difficulty_reason"),
    difficultyAttempts: smallint("difficulty_attempts").default(0).notNull(),
    difficultyCallFailures: smallint("difficulty_call_failures").default(0).notNull(),
  },
  (table) => [
    index("move_features_pending_maia_idx")
      .using(
        "btree",
        table.difficultyCallFailures.asc().nullsLast(),
        table.gameId.asc().nullsLast(),
        table.ply.asc().nullsLast(),
      )
      .where(sql`(difficulty_status = 'pending_maia'::text)`),
    foreignKey({
      columns: [table.gameId],
      foreignColumns: [games.id],
      name: "move_features_game_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.gameId, table.ply], name: "move_features_pkey" }),
    check(
      "move_features_difficulty_status_check",
      sql`(difficulty_status IS NULL) OR (difficulty_status = ANY (ARRAY['ok'::text, 'pending_maia'::text, 'not_applicable'::text]))`,
    ),
    check(
      "move_features_difficulty_ok_check",
      sql`(difficulty_status IS NULL) OR ((difficulty_status = 'ok'::text) = ((p_best IS NOT NULL) AND (p_ok IS NOT NULL) AND (difficulty_model IS NOT NULL)))`,
    ),
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
    difficultyPendingAt: timestamp("difficulty_pending_at", { withTimezone: true, mode: "date" }),
    whiteSelected: boolean("white_selected").notNull(),
    blackSelected: boolean("black_selected").notNull(),
  },
  (table) => [
    index("reference_games_build_status_idx").using(
      "btree",
      table.buildId.asc().nullsLast(),
      table.status.asc().nullsLast(),
    ),
    index("reference_games_difficulty_pending_idx")
      .using("btree", table.difficultyPendingAt.asc().nullsLast(), table.id.asc().nullsLast())
      .where(sql`(difficulty_pending_at IS NOT NULL)`),
    foreignKey({
      columns: [table.buildId],
      foreignColumns: [referenceBuilds.id],
      name: "reference_games_build_id_fkey",
    }).onDelete("cascade"),
    unique("reference_games_build_game_uq").on(table.buildId, table.lichessGameId),
    check("reference_games_a_side_is_selected", sql`white_selected OR black_selected`),
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

export const reportRuns = pgTable(
  "report_runs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    trigger: reportTrigger().notNull(),
    plan: aiPlan().notNull(),
    mode: reportMode().default("sync").notNull(),
    status: reportRunStatus().default("queued").notNull(),
    retryCount: integer("retry_count").default(0).notNull(),
    lastError: text("last_error"),
    nextAttemptAt: timestamp("next_attempt_at", { withTimezone: true, mode: "date" }),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "date" }),
    heartbeatAt: timestamp("heartbeat_at", { withTimezone: true, mode: "date" }),
    finishedAt: timestamp("finished_at", { withTimezone: true, mode: "date" }),
    promptVersion: text("prompt_version").notNull(),
    promptHash: text("prompt_hash"),
    inputHash: text("input_hash"),
    input: jsonb(),
    isFirstReport: boolean("is_first_report").notNull(),
    reportId: uuid("report_id"),
    aiDisabledReason: text("ai_disabled_reason"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    reconcileCount: smallint("reconcile_count").default(0).notNull(),
  },
  (table) => [
    index("report_runs_account_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.timeClass.asc().nullsLast(),
      table.createdAt.desc().nullsFirst(),
    ),
    index("report_runs_live_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(
        sql`(status = ANY (ARRAY['queued'::report_run_status, 'running'::report_run_status, 'awaiting_retry'::report_run_status, 'awaiting_batch'::report_run_status]))`,
      ),
    index("report_runs_status_idx").using(
      "btree",
      table.status.asc().nullsLast(),
      table.nextAttemptAt.asc().nullsLast(),
    ),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "report_runs_account_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const reportSections = pgTable(
  "report_sections",
  {
    reportId: uuid("report_id").notNull(),
    section: text().notNull(),
    status: aiItemStatus().notNull(),
    content: jsonb(),
    dropped: jsonb().default([]).notNull(),
    attempts: smallint().notNull(),
    model: text().notNull(),
    validatedAt: timestamp("validated_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.reportId],
      foreignColumns: [reports.id],
      name: "report_sections_report_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.reportId, table.section], name: "report_sections_pkey" }),
    check(
      "report_sections_section_check",
      sql`section = ANY (ARRAY['overview'::text, 'weaknesses'::text, 'openings'::text, 'climb_plan'::text, 'areas'::text])`,
    ),
    check(
      "report_sections_status_check",
      sql`status = ANY (ARRAY['ready'::ai_item_status, 'dropped'::ai_item_status])`,
    ),
  ],
);

export const reports = pgTable(
  "reports",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    version: integer().notNull(),
    reportRunId: uuid("report_run_id").notNull(),
    schemaVersion: text("schema_version").notNull(),
    computed: jsonb().notNull(),
    status: reportStatus().notNull(),
    aiDisclosure: jsonb("ai_disclosure"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "reports_account_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.reportRunId],
      foreignColumns: [reportRuns.id],
      name: "reports_report_run_id_fkey",
    }),
    unique("reports_account_id_time_class_version_key").on(
      table.accountId,
      table.timeClass,
      table.version,
    ),
    unique("reports_report_run_id_key").on(table.reportRunId),
  ],
);

export const session = pgTable(
  "session",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    expiresAt: timestamp("expires_at", { withTimezone: true, mode: "date" }).notNull(),
    token: text().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    ipAddress: text("ip_address"),
    userAgent: text("user_agent"),
    userId: uuid("user_id").notNull(),
  },
  (table) => [
    index("session_user_id_idx").using("btree", table.userId.asc().nullsLast()),
    foreignKey({
      columns: [table.userId],
      foreignColumns: [user.id],
      name: "session_user_id_user_id_fk",
    }).onDelete("cascade"),
    unique("session_token_unique").on(table.token),
  ],
);

export const user = pgTable(
  "user",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    name: text().notNull(),
    email: text().notNull(),
    emailVerified: boolean("email_verified").default(false).notNull(),
    image: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    birthMonth: text("birth_month"),
    ageCheckedAt: timestamp("age_checked_at", { withTimezone: true, mode: "date" }),
    marketingEmail: boolean("marketing_email").default(false).notNull(),
    publicShareDefault: boolean("public_share_default").default(false).notNull(),
  },
  (table) => [
    unique("user_email_unique").on(table.email),
    check("user_email_lower", sql`email = lower(email)`),
    check(
      "user_birth_month_format",
      sql`(birth_month IS NULL) OR (birth_month ~ '^[0-9]{4}-(0[1-9]|1[0-2])$'::text)`,
    ),
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
    userId: uuid("user_id"),
  },
  (table) => [
    index("user_events_account_id_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.id.asc().nullsLast(),
    ),
    index("user_events_created_idx").using("btree", table.createdAt.asc().nullsLast()),
    index("user_events_user_idx")
      .using("btree", table.userId.asc().nullsLast(), table.id.asc().nullsLast())
      .where(sql`(user_id IS NOT NULL)`),
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

export const verification = pgTable(
  "verification",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    identifier: text().notNull(),
    value: text().notNull(),
    expiresAt: timestamp("expires_at", { withTimezone: true, mode: "date" }).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
  },
  (table) => [
    index("verification_identifier_idx").using("btree", table.identifier.asc().nullsLast()),
  ],
);

export const weaknessChangeNotes = pgTable(
  "weakness_change_notes",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    changeKey: text("change_key").notNull(),
    seriesKey: text("series_key").notNull(),
    onsetDate: date("onset_date"),
    status: aiItemStatus().default("pending").notNull(),
    input: jsonb().notNull(),
    text: text(),
    segments: jsonb(),
    citations: jsonb().default([]).notNull(),
    model: text(),
    retryCount: integer("retry_count").default(0).notNull(),
    nextAttemptAt: timestamp("next_attempt_at", { withTimezone: true, mode: "date" }),
    aiDisclosure: jsonb("ai_disclosure"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    reconcileCount: smallint("reconcile_count").default(0).notNull(),
  },
  (table) => [
    index("weakness_change_notes_pending_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(sql`(status = 'pending'::ai_item_status)`),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weakness_change_notes_account_id_fkey",
    }).onDelete("cascade"),
    unique("weakness_change_notes_account_id_change_key_key").on(table.accountId, table.changeKey),
  ],
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
    side: sideScope().default("both").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weakness_estimates_account_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.accountId, table.timeClass, table.seriesKey, table.side],
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

export const weeklyReviews = pgTable(
  "weekly_reviews",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    timeClass: timeClass("time_class").notNull(),
    weekStart: date("week_start").notNull(),
    status: aiItemStatus().default("pending").notNull(),
    mode: reportMode().default("sync").notNull(),
    input: jsonb().notNull(),
    content: jsonb(),
    dropped: jsonb().default([]).notNull(),
    model: text(),
    promptHash: text("prompt_hash"),
    retryCount: integer("retry_count").default(0).notNull(),
    nextAttemptAt: timestamp("next_attempt_at", { withTimezone: true, mode: "date" }),
    aiDisclosure: jsonb("ai_disclosure"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    reconcileCount: smallint("reconcile_count").default(0).notNull(),
  },
  (table) => [
    index("weekly_reviews_pending_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(sql`(status = 'pending'::ai_item_status)`),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "weekly_reviews_account_id_fkey",
    }).onDelete("cascade"),
    unique("weekly_reviews_account_id_time_class_week_start_key").on(
      table.accountId,
      table.timeClass,
      table.weekStart,
    ),
  ],
);
