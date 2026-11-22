export { createDb, type Db, type DbOptions } from "./client.js";
export {
  EVENT_RETENTION_MS,
  type EventsPage,
  eventVisibleTo,
  insertUserEvent,
  listEventsAfter,
  listEventsPage,
  MAX_NOTIFY_BYTES,
  notificationJson,
  type OwnedUserEventRow,
  oldestEventId,
  type StreamEvent,
  USER_EVENTS_CHANNEL,
  USER_EVENTS_LOCK_KEY,
  type UserEventRow,
  userEventsChannel,
} from "./events.js";
export { type AnalysisStatus, type NewGame, upsertGame } from "./games.js";
// Web auth tables (ADR-WEB-3) come from the schema generated from db/migrations (ADR 0033).
export {
  account,
  accountLinks,
  chesscomVerifications,
  invites,
  linkMethod,
  session,
  user,
  verification,
} from "./generated/schema.js";
export { migrationsDir, runMigrations } from "./migrate.js";
export * from "./schema.js";
export { assertSafeTestDatabase } from "./test-guard.js";
