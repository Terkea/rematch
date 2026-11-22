import { assertSafeTestDatabase, createDb, runMigrations } from "@ci/db";
import { sql } from "drizzle-orm";
import postgres from "postgres";

// Run by the Playwright globalSetup (through tsx, so @ci/db resolves to source). The e2e web
// service uses its own database (`E2E_DATABASE_URL`, which must end in _test), never the dev
// `chess` database that holds analysed games. Creates it if missing, migrates it, and empties
// every table the e2e specs touch, so each run starts from nothing.

const url = assertSafeTestDatabase(process.env.E2E_DATABASE_URL);
const name = new URL(url).pathname.slice(1);
const adminUrl = new URL(url);
adminUrl.pathname = "/postgres";

const admin = postgres(adminUrl.toString(), { max: 1, onnotice: () => {} });
const [existing] = await admin`select 1 as found from pg_database where datname = ${name}`;
if (!existing) await admin.unsafe(`create database "${name.replaceAll('"', "")}"`);
await admin.end();

const db = createDb(url);
await runMigrations(db);
await db.execute(sql`
  truncate table "user", verification, invites, magic_link_sends, account_links,
    chesscom_verifications, chesscom_request_budgets, chesscom_profile_requests, linked_accounts
  restart identity cascade
`);
console.log(`e2e database ${name}: migrated and emptied`);
process.exit(0);
