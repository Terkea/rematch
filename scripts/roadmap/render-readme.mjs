#!/usr/bin/env node
// Writes the README's roadmap summary (between the roadmap markers) from
// .github/settings/milestones.json and roadmap-issues.json (ADR 0099).
//   node scripts/roadmap/render-readme.mjs           # rewrite README.md
//   node scripts/roadmap/render-readme.mjs --check   # fail when README.md is stale (CI)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { loadSeed, renderReadmeBlock, replaceBlock } from "./roadmap-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const readmePath = join(root, "README.md");
// A Windows checkout may have CRLF line ends; compare and write LF.
const current = readFileSync(readmePath, "utf8").replaceAll("\r\n", "\n");
const next = replaceBlock(current, renderReadmeBlock(loadSeed(root)));

if (process.argv.includes("--check")) {
  if (next !== current) {
    console.error(
      "README.md's roadmap summary is stale. Run: node scripts/roadmap/render-readme.mjs and commit the result.",
    );
    process.exit(1);
  }
  console.log("README.md's roadmap summary is current.");
} else if (next !== current) {
  writeFileSync(readmePath, next);
  console.log("README.md's roadmap summary updated.");
} else {
  console.log("README.md's roadmap summary is already current.");
}
