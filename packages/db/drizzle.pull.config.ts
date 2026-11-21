import { defineConfig } from "drizzle-kit";

// `drizzle-kit pull` introspects a database migrated from db/migrations/ (ADR 0033). Run it through
// scripts/db/generate-drizzle-schema.sh, which supplies DATABASE_URL; never point it at a real database.
const url = process.env.DATABASE_URL;
if (!url) throw new Error("DATABASE_URL is required (use scripts/db/generate-drizzle-schema.sh)");

export default defineConfig({
  dialect: "postgresql",
  out: "./src/generated",
  dbCredentials: { url },
  schemaFilter: ["public"],
  // sqlx bookkeeping is not part of the application schema.
  tablesFilter: ["!_sqlx_migrations"],
});
