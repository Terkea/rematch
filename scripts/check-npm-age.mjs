#!/usr/bin/env node
// npm counterpart of backend/scripts/check-crate-age.sh (ADR 0031).
// Fails when (a) any version resolved in pnpm-lock.yaml was published less than
// 7 days ago, or (b) a direct dependency in a workspace package.json is not an
// exact version. `workspace:` and `catalog:` specifiers are allowed.
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MIN_AGE_DAYS = 7;
const CACHE_DIR = join(ROOT, ".cache", "npm-age");
const USER_AGENT = "chess-improver-ci (+https://github.com/chess-improver)";
const CONCURRENCY = 8;
const DEP_FIELDS = ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"];

// ---- (b) exact pins -------------------------------------------------------
function packageJsonFiles() {
  const out = [join(ROOT, "package.json")];
  for (const group of ["apps", "packages", "services"]) {
    const dir = join(ROOT, group);
    if (!existsSync(dir)) continue;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const f = join(dir, e.name, "package.json");
      if (e.isDirectory() && existsSync(f)) out.push(f);
    }
  }
  return out;
}

const RANGE = /^(\^|~|\*|>|<|=|x$|latest$)|[\s|]|\.x\b|^$/;
const isAllowedSpec = (s) => s.startsWith("workspace:") || s.startsWith("catalog:");

const rangeViolations = [];
for (const file of packageJsonFiles()) {
  const pkg = JSON.parse(readFileSync(file, "utf8"));
  for (const field of DEP_FIELDS) {
    for (const [name, spec] of Object.entries(pkg[field] ?? {})) {
      if (isAllowedSpec(spec)) continue;
      if (RANGE.test(spec))
        rangeViolations.push(`${file.slice(ROOT.length + 1)}: ${field}.${name} = "${spec}"`);
    }
  }
}

// ---- (a) release age ------------------------------------------------------
// Lockfile v9 `packages:` keys look like `name@1.2.3` or `'@scope/name@1.2.3'`
// (peer suffixes only appear under `snapshots:`).
function lockedVersions() {
  const lines = readFileSync(join(ROOT, "pnpm-lock.yaml"), "utf8").split(/\r?\n/);
  const found = new Map(); // name -> Set(version)
  let inPackages = false;
  for (const line of lines) {
    if (/^\S/.test(line)) inPackages = line.startsWith("packages:");
    if (!inPackages) continue;
    const m = line.match(/^ {2}'?((?:@[^/@]+\/)?[^@'\s]+)@([^':\s(]+)'?:\s*$/);
    if (!m) continue;
    const [, name, version] = m;
    if (!found.has(name)) found.set(name, new Set());
    found.get(name).add(version);
  }
  return found;
}

function readCache(name) {
  try {
    return JSON.parse(readFileSync(join(CACHE_DIR, `${encodeURIComponent(name)}.json`), "utf8"));
  } catch {
    return {};
  }
}

async function publishTimes(name, versions) {
  const cached = readCache(name);
  if ([...versions].every((v) => cached[v])) return cached; // publish times are immutable
  let lastErr;
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(`https://registry.npmjs.org/${name.replace("/", "%2f")}`, {
        headers: { "user-agent": USER_AGENT, accept: "application/json" },
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const { time = {} } = await res.json();
      mkdirSync(CACHE_DIR, { recursive: true });
      writeFileSync(join(CACHE_DIR, `${encodeURIComponent(name)}.json`), JSON.stringify(time));
      return time;
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 500 * attempt));
    }
  }
  throw new Error(`${name}: registry lookup failed (${lastErr?.message})`);
}

const locked = lockedVersions();
const entries = [...locked.entries()];
const young = [];
const errors = [];
const now = Date.now();
let next = 0;
let checked = 0;
await Promise.all(
  Array.from({ length: CONCURRENCY }, async () => {
    while (next < entries.length) {
      const [name, versions] = entries[next++];
      try {
        const time = await publishTimes(name, versions);
        for (const v of versions) {
          checked++;
          if (!time[v]) {
            errors.push(`${name}@${v}: no publish time in registry`);
            continue;
          }
          const ageDays = (now - Date.parse(time[v])) / 86_400_000;
          if (ageDays < MIN_AGE_DAYS)
            young.push(`${name}@${v} published ${time[v]} (${ageDays.toFixed(1)} days old)`);
        }
      } catch (e) {
        errors.push(e.message);
      }
    }
  }),
);

console.log(`checked ${checked} locked versions of ${entries.length} packages`);
if (young.length)
  console.log(`\nYounger than ${MIN_AGE_DAYS} days:\n  ${young.sort().join("\n  ")}`);
if (errors.length) console.log(`\nLookup errors:\n  ${errors.sort().join("\n  ")}`);
if (rangeViolations.length)
  console.log(`\nDirect dependencies not pinned exactly:\n  ${rangeViolations.join("\n  ")}`);
if (young.length || errors.length || rangeViolations.length) process.exit(1);
console.log(
  "OK: all locked versions are old enough and all direct dependencies are pinned exactly.",
);
