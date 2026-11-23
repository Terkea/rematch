// The roadmap for the site, written to src/data/generated/roadmap.json at build time (ADR 0099).
// With `live`, it reads the milestones and epics from the public GitHub API; when that fails (the
// repository is private, the build is offline, the rate limit is spent) or GitHub has none of the
// roadmap's milestones yet, it falls back to the seed files in .github/settings. Never fatal.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fromGitHub, fromSeed, REPO } from "../../scripts/roadmap/roadmap-lib.mjs";

const API = "https://api.github.com";

async function getJson(fetchImpl, path, token) {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "rematch-site-build" };
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetchImpl(`${API}${path}`, { headers, signal: AbortSignal.timeout(10_000) });
  if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`);
  return res.json();
}

/** Every page of a list endpoint (at most 10, which is 1,000 items). */
async function getAll(fetchImpl, path, token) {
  const out = [];
  for (let page = 1; page <= 10; page += 1) {
    const sep = path.includes("?") ? "&" : "?";
    const items = await getJson(fetchImpl, `${path}${sep}per_page=100&page=${page}`, token);
    out.push(...items);
    if (items.length < 100) break;
  }
  return out;
}

/** The roadmap model, and a note saying where it came from. */
export async function loadRoadmap(seed, { live = false, fetchImpl = fetch, token } = {}) {
  if (!live) return { model: fromSeed(seed), note: "seed files (live fetch not requested)" };
  try {
    const milestones = await getAll(fetchImpl, `/repos/${REPO}/milestones?state=all`, token);
    const known = new Set(seed.milestones.map((m) => m.title));
    if (!milestones.some((m) => known.has(m.title))) {
      return { model: fromSeed(seed), note: "seed files (GitHub has no roadmap milestones yet)" };
    }
    const epics = await getAll(fetchImpl, `/repos/${REPO}/issues?state=all&labels=epic`, token);
    return { model: fromGitHub(seed, milestones, epics), note: `GitHub (${REPO})` };
  } catch (err) {
    return { model: fromSeed(seed), note: `seed files (GitHub unavailable: ${err.message})` };
  }
}

export function writeRoadmap(file, model) {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(model, null, 2)}\n`);
}
