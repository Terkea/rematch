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
 * `backfill` marks an open frontier left by a run that stopped at `--max`: everything above
 * `newestSeenAt` is imported and the next run continues below `oldestReached` with no upper
 * watermark bound.
 */
export interface ImportCursor {
  oldestReached: string | null;
  newestSeenAt: string | null;
  backfill?: boolean;
}

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

export const jobState = pgEnum("job_state", [
  "queued",
  "running",
  "done",
  "failed",
  "dead",
  "paused",
]);

export const judgement = pgEnum("judgement", ["inaccuracy", "mistake", "blunder"]);

export const linkMethod = pgEnum("link_method", [
  "username",
  "location_code",
  "lichess_oauth",
  "chesscom_oauth",
]);

export const phase = pgEnum("phase", ["opening", "middlegame", "endgame"]);

export const platform = pgEnum("platform", ["lichess", "chesscom"]);

export const result = pgEnum("result", ["1-0", "0-1", "1/2-1/2", "*"]);

export const timeClass = pgEnum("time_class", ["bullet", "blitz", "rapid", "classical", "daily"]);

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
  },
  (table) => [
    index("games_account_played_idx").using(
      "btree",
      table.accountId.asc().nullsLast(),
      table.playedAt.desc().nullsLast(),
    ),
    index("games_analysis_status_idx").using("btree", table.analysisStatus.asc().nullsLast()),
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
