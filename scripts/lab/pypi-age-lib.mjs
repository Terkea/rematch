// The logic of scripts/check-pypi-age.mjs, with no I/O, so scripts/lab/test-pypi-age.mjs can test
// it offline. The lab's Python packages follow the crates' and npm's rule (ADR 0031): every
// version in lab/uv.lock was published at least 7 days before it was adopted, and every direct
// dependency in lab/pyproject.toml is pinned exactly.

export const MIN_AGE_DAYS = 7;
const DAY_MS = 86_400_000;

/** The `[[package]]` entries of a uv.lock: name, version, source and the files' upload times. */
export function parseUvLock(text) {
  const out = [];
  for (const block of text.split(/^\[\[package\]\]\s*$/m).slice(1)) {
    const field = (k) => block.match(new RegExp(`^${k} = "([^"]*)"`, "m"))?.[1];
    const name = field("name");
    const version = field("version");
    if (!name || !version) continue;
    const source = block.match(/^source = \{ (\w+) = "([^"]*)"/m);
    const uploads = [...block.matchAll(/upload-time = "([^"]+)"/g)].map((m) => m[1]);
    out.push({
      name,
      version,
      sourceKind: source?.[1] ?? null,
      source: source?.[2] ?? null,
      uploads,
    });
  }
  return out;
}

/** The lock's `exclude-newer` cut-off, or null. */
export function excludeNewer(text) {
  return text.match(/^\[options\][\s\S]*?^exclude-newer = "([^"]+)"/m)?.[1] ?? null;
}

/** The requirement strings of `[project] dependencies` and every `[dependency-groups]` list. */
export function pyprojectRequirements(text) {
  const reqs = [];
  for (const section of text.split(/^\[/m)) {
    const header = section.slice(0, section.indexOf("]"));
    if (header !== "project" && header !== "dependency-groups") continue;
    for (const m of section.matchAll(/^[\w-]+ = \[([\s\S]*?)\]/gm))
      for (const q of m[1].matchAll(/"([^"]+)"/g)) reqs.push(q[1]);
  }
  return reqs;
}

/** Requirements that are not an exact `name==version` pin. */
export function pinViolations(requirements) {
  return requirements.filter((r) => !/^[A-Za-z0-9][A-Za-z0-9._-]*(\[[\w,-]+\])?==[\w.+!-]+$/.test(r));
}

/** The earliest upload time (ISO) among a PyPI release's files, or null. */
export function firstUpload(releaseJson) {
  const times = (releaseJson?.urls ?? [])
    .map((f) => f.upload_time_iso_8601)
    .filter(Boolean)
    .sort();
  return times[0] ?? null;
}

/**
 * Locked packages younger than `minDays` at `now` (ms). `published` maps `name@version` to an
 * ISO time; a package with no time is reported in `missing`.
 */
export function ageViolations(packages, published, now, minDays = MIN_AGE_DAYS) {
  const young = [];
  const missing = [];
  for (const p of packages) {
    const t = published.get(`${p.name}@${p.version}`);
    if (!t) {
      missing.push(`${p.name}@${p.version}`);
      continue;
    }
    const days = (now - Date.parse(t)) / DAY_MS;
    if (days < minDays)
      young.push(`${p.name}@${p.version} published ${t} (${days.toFixed(1)} days old)`);
  }
  return { young, missing };
}

/** Locked packages not from PyPI (a git, path or other index source). */
export function foreignSources(packages) {
  return packages
    .filter((p) => p.sourceKind !== "registry" || p.source !== "https://pypi.org/simple")
    .filter((p) => p.sourceKind !== "virtual" && p.sourceKind !== "editable")
    .map((p) => `${p.name}@${p.version} from ${p.sourceKind ?? "?"} ${p.source ?? ""}`.trim());
}
