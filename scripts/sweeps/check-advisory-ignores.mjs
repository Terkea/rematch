#!/usr/bin/env node
// Fails when an advisory ignore in backend/deny.toml has no "# expires: YYYY-MM-DD" comment, or
// is past that date (ADR 0099). cargo-deny keeps an ignore forever; this makes each one a dated
// exception the weekly sweep reports the day after it lapses.
//   node scripts/sweeps/check-advisory-ignores.mjs [--today YYYY-MM-DD]
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { advisoryIgnores } from "./sweeps-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const i = process.argv.indexOf("--today");
const today = i > 0 ? process.argv[i + 1] : new Date().toISOString().slice(0, 10);
const { entries, problems } = advisoryIgnores(
  readFileSync(join(root, "backend", "deny.toml"), "utf8"),
  today,
);
if (problems.length > 0) {
  console.log(`Advisory ignores in backend/deny.toml (${today}):`);
  for (const p of problems) console.log(`- ${p}`);
  process.exit(1);
}
const list = entries.map((e) => `${e.id} until ${e.expires}`).join(", ") || "none";
console.log(`Advisory ignores current on ${today}: ${list}.`);
