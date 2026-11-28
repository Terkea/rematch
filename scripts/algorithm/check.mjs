#!/usr/bin/env node
// The algorithm version guard (ADRs 0104 and 0105). Compares algorithm/manifest.json with the
// base's (ALGO_BASE, default origin/main) and fails when:
//   1. the manifest's id does not match its content (it was edited by hand);
//   2. a component changed and the version was not bumped as the policy says: major when layer 1
//      to 3 output can change (any facts, profile or recommendation field other than a source
//      digest, or new replay goldens), minor when only the language layer changed, patch when
//      only a source digest changed and the replay goldens did not;
//   3. algorithm/CHANGELOG.md has no `## <id>` heading for the manifest's id;
//   4. algorithm/MATH.md has no `## algo-X.Y.Z` section for a version in the changelog, so no
//      version ships without its maths (ADR 0118).
// That the manifest matches the code is the Rust test's job (backend/crates/algo/tests).
// Run: node scripts/algorithm/check.mjs. Its tests: node scripts/algorithm/test-check.mjs.
// No dependencies.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const MANIFEST = "algorithm/manifest.json";
export const CHANGELOG = "algorithm/CHANGELOG.md";
export const MATH = "algorithm/MATH.md";
export const GOLDENS = "backend/fixtures/replay";
/** The layers whose output the replay goldens hold. */
export const OUTPUT_LAYERS = ["facts", "profile", "recommendation"];

/** Canonical JSON: sorted keys, no whitespace (the same bytes as the Rust `algo::canonical`). */
export function canonical(v) {
  if (Array.isArray(v)) return `[${v.map(canonical).join(",")}]`;
  if (v !== null && typeof v === "object") {
    const keys = Object.keys(v).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonical(v[k])}`).join(",")}}`;
  }
  return JSON.stringify(v);
}

/** `algo-<version>+<first 12 hex of sha256(canonical {version, components})>`. */
export function idOf(m) {
  const hash = createHash("sha256")
    .update(canonical({ version: m.version, components: m.components }))
    .digest("hex");
  return `algo-${m.version}+${hash.slice(0, 12)}`;
}

/** Every leaf path that differs between a and b. */
export function changedPaths(a, b, path = "") {
  const isObj = (x) => x !== null && typeof x === "object" && !Array.isArray(x);
  if (isObj(a) && isObj(b)) {
    const keys = [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
    return keys.flatMap((k) => changedPaths(a[k], b[k], path ? `${path}.${k}` : k));
  }
  return canonical(a ?? null) === canonical(b ?? null) ? [] : [path];
}

const RANK = { none: 0, patch: 1, minor: 2, major: 3 };

/** The smallest bump the change needs: "none", "patch", "minor" or "major". */
export function requiredBump(base, head, goldensChanged) {
  if (goldensChanged) return "major";
  const paths = changedPaths(base.components, head.components);
  let need = "none";
  for (const p of paths) {
    const [layer, field] = p.split(".");
    const level = OUTPUT_LAYERS.includes(layer)
      ? field === "source_digest"
        ? "patch"
        : "major"
      : layer === "language"
        ? "minor"
        : "major";
    if (RANK[level] > RANK[need]) need = level;
  }
  return need;
}

function parse(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v ?? "");
  if (!m) throw new Error(`not a version: ${v}`);
  return m.slice(1).map(Number);
}

/** The bump from a to b: "none", "patch", "minor", "major", or "down" for a step back. */
export function bumpOf(a, b) {
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) {
    if (y[i] > x[i]) {
      const rest = y.slice(i + 1).every((n) => n === 0);
      if (!rest && i < 2) return "malformed";
      return ["major", "minor", "patch"][i];
    }
    if (y[i] < x[i]) return "down";
  }
  return "none";
}

/** The versions of the changelog's `## algo-X.Y.Z+<hash>` entries, in file order. */
export function changelogVersions(changelog) {
  return changelog
    .split(/\r?\n/)
    .map((l) => /^## algo-(\d+\.\d+\.\d+)\+[0-9a-f]+\s*$/.exec(l)?.[1])
    .filter((v) => v !== undefined);
}

/** The versions MATH.md has a `## algo-X.Y.Z` section for (a `+<hash>` suffix is allowed). */
export function mathVersions(math) {
  return math
    .split(/\r?\n/)
    .map((l) => /^## algo-(\d+\.\d+\.\d+)(?:\+[0-9a-f]+)?\s*$/.exec(l)?.[1])
    .filter((v) => v !== undefined);
}

/** One line per changelog version without a MATH.md section (or one line: no MATH.md). */
export function missingMath(changelog, math) {
  if (math === null) return [`${MATH} does not exist`];
  const have = new Set(mathVersions(math));
  return changelogVersions(changelog)
    .filter((v) => !have.has(v))
    .map((v) => `${MATH} has no \`## algo-${v}\` section (the maths of that version)`);
}

/**
 * Every problem, as readable lines (empty: the check passes). `math` is MATH.md's text, or null
 * when the file is missing; left undefined, the maths is not checked.
 */
export function check({ base, head, changelog, goldensChanged, math }) {
  const errors = [];
  if (math !== undefined) errors.push(...missingMath(changelog, math));
  const id = idOf(head);
  if (head.id !== id)
    errors.push(
      `${MANIFEST}: its id ${head.id} does not match its content (${id}); regenerate it with \`cargo run -p cli -- algo manifest --write\``,
    );
  if (!changelog.split(/\r?\n/).includes(`## ${head.id}`)) {
    errors.push(`${CHANGELOG} has no \`## ${head.id}\` entry`);
  }
  if (!base) return errors;
  const need = requiredBump(base, head, goldensChanged);
  const got = bumpOf(base.version, head.version);
  if (got === "down" || got === "malformed") {
    errors.push(`version ${base.version} -> ${head.version} is not a forward semver bump`);
  } else if (RANK[got] < RANK[need]) {
    const why = goldensChanged
      ? "the replay goldens changed"
      : changedPaths(base.components, head.components).join(", ");
    errors.push(
      `the algorithm changed (${why}) and needs at least a ${need} bump, but the version went ${base.version} -> ${head.version}`,
    );
  }
  return errors;
}

function git(args, root) {
  return execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
  const head = JSON.parse(readFileSync(join(root, MANIFEST), "utf8"));
  const changelog = readFileSync(join(root, CHANGELOG), "utf8");
  const mathPath = join(root, MATH);
  const math = existsSync(mathPath) ? readFileSync(mathPath, "utf8") : null;
  const ref = process.env.ALGO_BASE ?? "origin/main";
  let base = null;
  let goldensChanged = false;
  try {
    base = JSON.parse(git(["show", `${ref}:${MANIFEST}`], root));
    goldensChanged = git(["diff", "--name-only", ref, "--", GOLDENS], root).trim() !== "";
  } catch {
    console.log(`algorithm check: no manifest at ${ref}; checking the manifest on its own`);
  }
  const errors = check({ base, head, changelog, goldensChanged, math });
  if (errors.length > 0) {
    for (const e of errors) console.error(`algorithm check: ${e}`);
    process.exit(1);
  }
  console.log(`algorithm check: ${head.id} ok${base ? ` (base ${base.id})` : ""}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
