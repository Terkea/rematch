// The release pipeline's logic (ADR 0098), kept free of I/O so test-release.mjs can check it.
// scripts/release/release.mjs is the command-line front end the workflow calls.
import { ciCheckNames } from "../github/settings-lib.mjs";

/** GHCR namespace. Image names are lowercase; the GitHub owner is Terkea. */
export const REGISTRY = "ghcr.io/terkea";

/**
 * The images a self-hoster runs. Postgres and Redis are upstream images, pinned by digest in
 * deploy/compose.yaml. Maia (opt-in, ADR-R2-13) is not published: build it from the source.
 */
export const IMAGES = [
  { name: "web", dockerfile: "apps/web/Dockerfile", target: "runtime" },
  { name: "worker", dockerfile: "backend/Dockerfile", target: "worker" },
  { name: "ai-sidecar", dockerfile: "services/ai-sidecar/Dockerfile", target: "runtime" },
].map((image) => ({ ...image, repository: `${REGISTRY}/rematch-${image.name}` }));

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

/** `v0.1.0` -> its parts. Only a `v`-prefixed SemVer tag is a release tag. */
export function parseTag(tag) {
  if (typeof tag !== "string" || !tag.startsWith("v")) {
    throw new Error(`release tag "${tag}" must start with v (v0.1.0)`);
  }
  const m = SEMVER.exec(tag.slice(1));
  if (!m) throw new Error(`release tag "${tag}" is not v + SemVer (v0.1.0)`);
  return {
    version: tag.slice(1),
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    pre: m[4] ?? "",
  };
}

/**
 * The image tags for a release tag. Below 1.0 every release is a beta: X.Y.Z, X.Y and the moving
 * `beta`, never `latest`. From 1.0: X.Y.Z, X.Y, X and `latest`. A prerelease (`-rc.1`) gets only
 * its own tag, so it never moves a channel.
 */
export function imageTags(tag) {
  const t = parseTag(tag);
  if (t.pre) return [t.version];
  const minor = `${t.major}.${t.minor}`;
  return t.major === 0 ? [t.version, minor, "beta"] : [t.version, minor, `${t.major}`, "latest"];
}

/**
 * The checks a release waits for: the ruleset's required checks that ci.yml reports. pr-title
 * runs on pull requests only, so it never reports on the tagged commit and is left out.
 */
export function requiredCiChecks(ruleset, ciYaml) {
  const rule = ruleset.rules.find((r) => r.type === "required_status_checks");
  const required = rule.parameters.required_status_checks.map((c) => c.context);
  const ci = new Set(ciCheckNames(ciYaml));
  return required.filter((c) => ci.has(c)).sort();
}

/**
 * Whether the tagged commit's CI passed: for every required check, its newest check run (a re-run
 * supersedes the run before it) must be completed with `success`. `wait` while one is missing or
 * running; `fail` on anything else (failure, cancelled, skipped, timed_out, action_required).
 */
export function ciGateVerdict(required, checkRuns) {
  const lines = [];
  let state = "pass";
  for (const name of required) {
    const runs = checkRuns
      .filter((r) => r.name === name)
      .sort((a, b) => String(b.started_at).localeCompare(String(a.started_at)));
    const newest = runs[0];
    if (!newest) {
      lines.push(`${name}: not started`);
      if (state === "pass") state = "wait";
    } else if (newest.status !== "completed") {
      lines.push(`${name}: ${newest.status}`);
      if (state === "pass") state = "wait";
    } else if (newest.conclusion === "success") {
      lines.push(`${name}: success`);
    } else {
      lines.push(`${name}: ${newest.conclusion}`);
      state = "fail";
    }
  }
  return { state, detail: lines.join("; ") };
}

// One image line of deploy/compose.yaml. The registry prefix and the version can be overridden for
// the smoke test (local tags); the default version is what release-please bumps.
const IMAGE_LINE =
  /^(\s*)image: \$\{REMATCH_IMAGE_PREFIX:-ghcr\.io\/terkea\/rematch\}-([a-z-]+):\$\{REMATCH_VERSION:-([^}]+)\}(\s*# x-release-please-version)?\s*$/gm;

/** The Rematch images deploy/compose.yaml pulls, with their default version. */
export function composeImageRefs(compose) {
  return [...compose.replaceAll("\r\n", "\n").matchAll(IMAGE_LINE)].map((m) => ({
    image: m[2],
    version: m[3],
    marked: Boolean(m[4]),
  }));
}

/**
 * The compose file attached to a release: every Rematch image pinned to `version@digest`, the
 * digest the workflow pushed, so a release asset can never pull a moved tag.
 */
export function renderReleaseCompose(compose, { version, digests }) {
  const text = compose.replaceAll("\r\n", "\n");
  for (const image of IMAGES) {
    const d = digests[image.name];
    if (!d) throw new Error(`no digest for ${image.name}`);
    if (!/^sha256:[0-9a-f]{64}$/.test(d)) throw new Error(`${image.name}: bad digest "${d}"`);
  }
  return text.replace(IMAGE_LINE, (_line, indent, name) => {
    const image = IMAGES.find((i) => i.name === name);
    if (!image) throw new Error(`deploy/compose.yaml names an unknown image rematch-${name}`);
    return `${indent}image: ${image.repository}:${version}@${digests[name]}`;
  });
}

/** `{{name}}` placeholders; a placeholder without a value is an error, never left in. */
export function renderTemplate(template, vars) {
  return template.replaceAll(/\{\{\s*([a-z_]+)\s*\}\}/g, (_m, key) => {
    if (!(key in vars)) throw new Error(`release-notes template: no value for {{${key}}}`);
    return String(vars[key]);
  });
}

const HEADER_START = "<!-- rematch-header -->";
const HEADER_END = "<!-- /rematch-header -->";

/** Puts the header above release-please's notes, replacing an earlier header (re-runs). */
export function mergeReleaseNotes(body, header) {
  let rest = (body ?? "").replaceAll("\r\n", "\n");
  const start = rest.indexOf(HEADER_START);
  const end = rest.indexOf(HEADER_END);
  if (start !== -1 && end > start) {
    rest = (rest.slice(0, start) + rest.slice(end + HEADER_END.length)).replace(/^\s+/, "");
  }
  return `${header.trim()}\n\n${rest}`;
}

/** deploy/stockfish-source.txt: `<sha256>  <file>  <url>` per line, `#` comments. */
export function parseSourceManifest(text) {
  return text
    .replaceAll("\r\n", "\n")
    .split("\n")
    .filter((l) => l.trim() && !l.trim().startsWith("#"))
    .map((l) => {
      const [sha256, file, url, ...extra] = l.trim().split(/\s+/);
      if (!/^[0-9a-f]{64}$/.test(sha256 ?? "") || !file || !url || extra.length > 0) {
        throw new Error(`stockfish-source.txt: "${l}" is not "<SHA-256>  <file>  <url>"`);
      }
      return { sha256, file, url };
    });
}
