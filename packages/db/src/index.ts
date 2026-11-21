export { createDb, type Db, type DbOptions } from "./client.js";
export {
  insertUserEvent,
  listEventsAfter,
  USER_EVENTS_LOCK_KEY,
  type UserEventRow,
} from "./events.js";
export { type AnalysisStatus, type NewGame, upsertGame } from "./games.js";
// Web auth tables (ADR-WEB-3) come from the schema generated from db/migrations (ADR 0033).
export { account, invites, session, user, verification } from "./generated/schema.js";
export { migrationsDir, runMigrations } from "./migrate.js";
export * from "./schema.js";
export { assertSafeTestDatabase } from "./test-guard.js";
