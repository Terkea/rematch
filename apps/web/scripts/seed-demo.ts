import { parseArgs } from "node:util";
import { createDb } from "@ci/db";
import { removeDemo, seedDemo } from "./demo-seed";
import { assertLocal } from "./dev-link-account";

// `pnpm seed:demo [--email you@example.com] [--remove]` (ADR 0097), run by scripts/seed-demo.sh
// inside the compose web container, where DATABASE_URL is the dev `chess` database. Local only:
// it refuses production and any database host that is not this machine or compose.

const { values } = parseArgs({
  options: { email: { type: "string" }, remove: { type: "boolean", default: false } },
});
const env = { NODE_ENV: process.env.NODE_ENV, DATABASE_URL: process.env.DATABASE_URL ?? "" };
assertLocal(env);
const db = createDb(env.DATABASE_URL);

if (values.remove) {
  const n = await removeDemo(db);
  console.log(
    n > 0 ? "seed:demo: the demo player is removed" : "seed:demo: no demo data to remove",
  );
  process.exit(0);
}

const email = values.email ?? process.env.INVITE_ALLOWLIST?.split(",")[0]?.trim();
if (!email) {
  console.error("seed:demo: pass --email you@example.com (or set INVITE_ALLOWLIST in .env)");
  process.exit(1);
}
const out = await seedDemo(db, { email });
console.log(
  `seed:demo: ${out.games} analysed games of a fictional player, linked to ${email} ` +
    `(${out.drills} trainer positions). Sign in with that address; the app shows "Demo data". ` +
    "Remove it with: pnpm seed:demo --remove",
);
process.exit(0);
