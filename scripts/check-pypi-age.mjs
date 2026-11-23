#!/usr/bin/env node
// PyPI counterpart of scripts/check-npm-age.mjs and backend/scripts/check-crate-age.sh (ADR 0031,
// ADR 0106). Fails when (a) a version in lab/uv.lock was published less than 7 days ago, (b) a
// direct dependency in lab/pyproject.toml is not an exact `==` pin, (c) a locked package comes
// from anywhere but PyPI, or (d) the lock has no `exclude-newer` cut-off.
//
// Publish times come from PyPI's JSON API (the earliest file of the release) and are cached in
// .cache/pypi-age, because they never change. PYPI_AGE_OFFLINE=1 makes no network call and trusts
// the upload times recorded in the lock instead (for a machine with no network).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ageViolations,
  excludeNewer,
  firstUpload,
  foreignSources,
  MIN_AGE_DAYS,
  parseUvLock,
  pinViolations,
  pyprojectRequirements,
} from "./lab/pypi-age-lib.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CACHE_DIR = join(ROOT, ".cache", "pypi-age");
const USER_AGENT = "rematch/0.1 (+https://github.com/Terkea/rematch)";
const CONCURRENCY = 8;
const OFFLINE = process.env.PYPI_AGE_OFFLINE === "1";

const lockText = readFileSync(join(ROOT, "lab", "uv.lock"), "utf8");
const pyproject = readFileSync(join(ROOT, "lab", "pyproject.toml"), "utf8");
const packages = parseUvLock(lockText).filter(
  (p) => p.sourceKind !== "virtual" && p.sourceKind !== "editable",
);

const cacheFile = (key) => join(CACHE_DIR, `${encodeURIComponent(key)}.json`);
function cached(key) {
  try {
    return JSON.parse(readFileSync(cacheFile(key), "utf8")).published ?? null;
  } catch {
    return null;
  }
}

async function publishTime(p) {
  const key = `${p.name}@${p.version}`;
  if (OFFLINE) return [...p.uploads].sort()[0] ?? null;
  const hit = cached(key);
  if (hit) return hit;
  let lastErr;
  for (let attempt = 1; attempt <= 4; attempt++) {
    try {
      const res = await fetch(`https://pypi.org/pypi/${p.name}/${p.version}/json`, {
        headers: { "user-agent": USER_AGENT, accept: "application/json" },
      });
      if (res.status === 404) return null;
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const published = firstUpload(await res.json());
      if (published) {
        mkdirSync(CACHE_DIR, { recursive: true });
        writeFileSync(cacheFile(key), JSON.stringify({ published }));
      }
      return published;
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 750 * attempt));
    }
  }
  throw new Error(`${key}: PyPI lookup failed (${lastErr?.message})`);
}

const published = new Map();
const errors = [];
let next = 0;
await Promise.all(
  Array.from({ length: CONCURRENCY }, async () => {
    while (next < packages.length) {
      const p = packages[next++];
      try {
        const t = await publishTime(p);
        if (t) published.set(`${p.name}@${p.version}`, t);
      } catch (e) {
        errors.push(e.message);
      }
    }
  }),
);

const { young, missing } = ageViolations(packages, published, Date.now());
const pins = pinViolations(pyprojectRequirements(pyproject));
const foreign = foreignSources(packages);
const cutoff = excludeNewer(lockText);

console.log(
  `checked ${packages.length} locked versions${OFFLINE ? " (offline: the lock's upload times)" : ""}`,
);
const report = (title, list) =>
  list.length && console.log(`\n${title}:\n  ${list.sort().join("\n  ")}`);
report(`Younger than ${MIN_AGE_DAYS} days`, young);
report("No publish time found", missing);
report("Lookup errors", errors);
report("Direct dependencies not pinned exactly", pins);
report("Not from PyPI", foreign);
if (!cutoff) console.log("\nlab/uv.lock has no exclude-newer cut-off ([tool.uv] exclude-newer)");
if (young.length || missing.length || errors.length || pins.length || foreign.length || !cutoff)
  process.exit(1);
console.log(
  `OK: every locked version is at least ${MIN_AGE_DAYS} days old, the direct dependencies are pinned exactly, and the lock stops at ${cutoff}.`,
);
