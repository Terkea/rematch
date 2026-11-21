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

export const aiPlan = pgEnum("ai_plan", ["free", "pro"]);

export const analysisStatus = pgEnum("analysis_status", [
  "pending",
  "analysing",
  "done",
  "failed",
  "skipped",
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

export const phase = pgEnum("phase", ["opening", "middlegame", "endgame"]);

export const platform = pgEnum("platform", ["lichess", "chesscom"]);

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

export const timeClass = pgEnum("time_class", ["bullet", "blitz", "rapid", "classical", "daily"]);

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
  },
  (table) => [
    index("ai_usage_account_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.createdAt.asc().nullsLast(),
    ),
    index("ai_usage_created_idx").using("btree", table.createdAt.asc().nullsLast()),
  ],
);

export const coachConversations = pgTable(
  "coach_conversations",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    accountId: uuid("account_id").notNull(),
    userId: uuid("user_id"),
    timeClass: timeClass("time_class").notNull(),
    title: text(),
    anchor: jsonb(),
    summary: text(),
    summaryThroughSeq: integer("summary_through_seq").default(0).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "date" }).defaultNow().notNull(),
    lastMessageAt: timestamp("last_message_at", { withTimezone: true, mode: "date" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("coach_conversations_owner_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.lastMessageAt.desc().nullsFirst(),
    ),
    index("coach_conversations_user_idx")
      .using("btree", table.userId.asc().nullsLast())
      .where(sql`(user_id IS NOT NULL)`),
    foreignKey({
      columns: [table.accountId],
      foreignColumns: [linkedAccounts.id],
      name: "coach_conversations_account_id_fkey",
    }).onDelete("cascade"),
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
  },
  (table) => [
    index("coach_messages_pending_idx")
      .using("btree", table.createdAt.asc().nullsLast())
      .where(sql`(status = 'pending'::chat_status)`),
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
  },
  (table) => [
    index("games_account_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.playedAt.desc().nullsLast(),
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
  },
  (table) => [index("job_metrics_created_idx").using("btree", table.createdAt.asc().nullsLast())],
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
  },
  (table) => [unique("linked_accounts_platform_username_uq").on(table.platform, table.username)],
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
