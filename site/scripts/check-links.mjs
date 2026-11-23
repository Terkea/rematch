#!/usr/bin/env node
// Local link check, no network (run after `astro build`):
// - every internal link and asset in site/dist (paths under /rematch/, plus #anchors) must exist;
// - every relative link and image in the root README.md must exist in the repository, and every
//   link to the Pages site must exist in site/dist.
// Paths that the open-source foundation lane adds (CONTRIBUTING.md and friends) count as present
// when that branch has them, so this check passes before and after the two lanes merge.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ROOT = resolve(SITE, "..");
const DIST = join(SITE, "dist");
const BASE = "/rematch/";
const PAGES = "https://terkea.github.io/rematch/";
const FOUNDATION_BRANCH = "chore/open-source-foundation";

if (!existsSync(DIST)) {
  console.error("site/dist is missing: run `pnpm --filter @ci/site build` first.");
  process.exit(1);
}

function walk(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

const htmlFiles = walk(DIST).filter((f) => f.endsWith(".html"));
const idsCache = new Map();

/** The dist file a site path resolves to, or null. */
function distFile(sitePath) {
  const rel = decodeURIComponent(sitePath.slice(BASE.length));
  const p = join(DIST, rel);
  if (existsSync(p) && statSync(p).isFile()) return p;
  const index = join(p, "index.html");
  if (existsSync(index)) return index;
  return null;
}

function idsOf(file) {
  let ids = idsCache.get(file);
  if (!ids) {
    const html = readFileSync(file, "utf8");
    ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
    idsCache.set(file, ids);
  }
  return ids;
}

const problems = [];
let checked = 0;

function checkSiteUrl(url, fromFile, from) {
  const [path, anchor] = url.split("#");
  const file = path === "" ? fromFile : distFile(path);
  checked++;
  if (!file) {
    problems.push(`${from}: missing ${url}`);
    return;
  }
  if (anchor && file.endsWith(".html") && !idsOf(file).has(decodeURIComponent(anchor))) {
    problems.push(`${from}: missing anchor ${url}`);
  }
}

for (const file of htmlFiles) {
  const html = readFileSync(file, "utf8");
  const from = posix.relative(DIST.replace(/\\/g, "/"), file.replace(/\\/g, "/"));
  for (const m of html.matchAll(/\s(?:href|src|poster)="([^"]+)"/g)) {
    const url = m[1].replace(/&amp;/g, "&");
    if (url.startsWith(BASE)) checkSiteUrl(url, file, from);
    else if (url.startsWith("#") && url.length > 1) checkSiteUrl(url, file, from);
    else if (url.startsWith("/") && !url.startsWith("//")) {
      problems.push(`${from}: link outside the base path ${url}`);
    }
  }
}

// ---- README ----
function inFoundationBranch(path) {
  try {
    execFileSync("git", ["cat-file", "-e", `${FOUNDATION_BRANCH}:${path}`], {
      cwd: ROOT,
      stdio: "ignore",
    });
    return true;
  } catch {
    return false;
  }
}

const readme = readFileSync(join(ROOT, "README.md"), "utf8").replace(/```[\s\S]*?```/g, "");
const targets = [
  ...[...readme.matchAll(/\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)].map((m) => m[1]),
  ...[...readme.matchAll(/\s(?:href|src)="([^"]+)"/g)].map((m) => m[1]),
];
for (const raw of targets) {
  const url = raw.replace(/&amp;/g, "&");
  if (url.startsWith(PAGES)) {
    checkSiteUrl(BASE + url.slice(PAGES.length), null, "README.md");
    continue;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(url) || url.startsWith("#")) continue;
  const path = posix.normalize(url.split("#")[0] ?? "");
  checked++;
  if (existsSync(join(ROOT, path))) continue;
  if (inFoundationBranch(path)) continue;
  problems.push(`README.md: missing ${url}`);
}

if (problems.length) {
  console.error(problems.join("\n"));
  console.error(`${problems.length} broken of ${checked} links checked`);
  process.exit(1);
}
console.log(`links: ${checked} checked in ${htmlFiles.length} pages and the README, 0 broken`);
