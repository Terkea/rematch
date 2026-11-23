#!/usr/bin/env node
// The stale-docs check of the weekly sweep (ADR 0099):
//   1. docs/decisions/README.md lists every decision record, and every record it lists exists;
//   2. every ADR that the docs, the README, CLAUDE.md and the site's pages mention exists.
// Prints the problems and exits 1 when there are any. No network.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { adrIdOf, adrIndexProblems, adrReferences } from "./sweeps-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const decisionsDir = join(root, "docs", "decisions");
const files = readdirSync(decisionsDir).filter((f) => f.endsWith(".md"));
const problems = adrIndexProblems(readFileSync(join(decisionsDir, "README.md"), "utf8"), files);

const known = new Set(files.map(adrIdOf).filter(Boolean));
// Pages generated from docs/ at build time are left out: their sources are scanned.
const SKIP = new Set(["node_modules", ".git", "dist", ".astro", "decisions", "research"]);
function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    if (SKIP.has(e) && dir.includes("site")) continue;
    if (e === "node_modules" || e === ".git") continue;
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(md|mdx)$/.test(e)) out.push(p);
  }
  return out;
}
const sources = [
  ...walk(join(root, "docs")),
  ...walk(join(root, "site", "src", "content")),
  ...["README.md", "CLAUDE.md", "CONTRIBUTING.md"].map((f) => join(root, f)).filter(existsSync),
];
for (const file of sources) {
  const missing = adrReferences(readFileSync(file, "utf8")).filter((id) => !known.has(id));
  for (const id of missing) {
    problems.push(
      `${relative(root, file).replaceAll("\\", "/")} mentions ADR ${id}, which does not exist`,
    );
  }
}

if (problems.length > 0) {
  console.log(`Stale docs (${problems.length}):`);
  for (const p of problems) console.log(`- ${p}`);
  process.exit(1);
}
console.log(
  `Docs current: ${files.length - 1} decision records indexed, ${sources.length} files' ADR references exist.`,
);
