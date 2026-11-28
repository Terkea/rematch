#!/usr/bin/env node
// Tests for scripts/algorithm/check.mjs (ADR 0105) on fixture manifests, plus the committed
// manifest and changelog. Run: node scripts/algorithm/test-check.mjs (no dependencies).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  bumpOf,
  CHANGELOG,
  canonical,
  changelogVersions,
  check,
  idOf,
  MANIFEST,
  MATH,
  mathVersions,
  missingMath,
  requiredBump,
} from "./check.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
let passed = 0;
function test(name, fn) {
  fn();
  passed++;
  console.log(`ok - ${name}`);
}

function manifest(version, patch = {}) {
  const components = {
    facts: { engine_protocol: "sf19-n300k-v1", engine_nodes: 300000, source_digest: { a: "1" } },
    profile: { detector_version: "tl-v1", source_digest: { b: "2" } },
    recommendation: { ranking_version: "rank-v1", rank_min_excess_per_game: 0.02 },
    language: { prompt_sets: { report: "r1" }, models: ["claude-sonnet-5-5"] },
  };
  for (const [path, value] of Object.entries(patch)) {
    const keys = path.split(".");
    let o = components;
    for (const k of keys.slice(0, -1)) o = o[k];
    o[keys.at(-1)] = value;
  }
  const m = { version, components };
  return { ...m, id: idOf(m) };
}
const entry = (m) => `# Changelog\n\n## ${m.id}\n\nText.\n`;

test("canonical JSON sorts keys and drops whitespace, as the Rust side does", () => {
  assert.equal(canonical({ b: 1, a: { z: "x", y: [1, 2] } }), '{"a":{"y":[1,2],"z":"x"},"b":1}');
  // The pinned id of algo::tests (backend/crates/algo/src/lib.rs).
  assert.equal(
    idOf({ version: "1.0.0", components: { b: 1, a: { z: "x", y: [1, 2] } } }),
    "algo-1.0.0+d34c96e0b472",
  );
});

test("semver bumps", () => {
  assert.equal(bumpOf("1.0.0", "1.0.0"), "none");
  assert.equal(bumpOf("1.0.0", "1.0.1"), "patch");
  assert.equal(bumpOf("1.0.3", "1.1.0"), "minor");
  assert.equal(bumpOf("1.2.3", "2.0.0"), "major");
  assert.equal(bumpOf("1.2.3", "2.1.0"), "malformed");
  assert.equal(bumpOf("2.0.0", "1.9.9"), "down");
});

test("the bump a change needs", () => {
  const base = manifest("1.0.0");
  assert.equal(requiredBump(base, manifest("1.0.0"), false), "none");
  assert.equal(
    requiredBump(base, manifest("1.0.0", { "facts.source_digest": { a: "9" } }), false),
    "patch",
  );
  assert.equal(
    requiredBump(base, manifest("1.0.0", { "language.prompt_sets": { report: "r2" } }), false),
    "minor",
  );
  assert.equal(
    requiredBump(base, manifest("1.0.0", { "facts.engine_nodes": 500000 }), false),
    "major",
  );
  assert.equal(
    requiredBump(
      base,
      manifest("1.0.0", { "recommendation.rank_min_excess_per_game": 0.03 }),
      false,
    ),
    "major",
  );
  assert.equal(
    requiredBump(base, manifest("1.0.0", { "facts.source_digest": { a: "9" } }), true),
    "major",
  );
});

test("an unchanged manifest with its changelog entry passes", () => {
  const m = manifest("1.0.0");
  assert.deepEqual(check({ base: m, head: m, changelog: entry(m), goldensChanged: false }), []);
});

test("a changed threshold without a version bump fails", () => {
  const base = manifest("1.0.0");
  const head = manifest("1.0.0", { "recommendation.rank_min_excess_per_game": 0.03 });
  const errors = check({ base, head, changelog: entry(head), goldensChanged: false });
  assert.equal(errors.length, 1);
  assert.match(errors[0], /needs at least a major bump/);
  assert.match(errors[0], /recommendation\.rank_min_excess_per_game/);
});

test("a minor bump is too small for a facts change, a major one is enough", () => {
  const base = manifest("1.0.0");
  const minor = manifest("1.1.0", { "facts.engine_nodes": 500000 });
  assert.equal(
    check({ base, head: minor, changelog: entry(minor), goldensChanged: false }).length,
    1,
  );
  const major = manifest("2.0.0", { "facts.engine_nodes": 500000 });
  assert.deepEqual(
    check({ base, head: major, changelog: entry(major), goldensChanged: false }),
    [],
  );
});

test("a prompt change takes a minor bump; a digest-only change a patch, unless goldens moved", () => {
  const base = manifest("1.0.0");
  const prompt = manifest("1.1.0", { "language.prompt_sets": { report: "r2" } });
  assert.deepEqual(
    check({ base, head: prompt, changelog: entry(prompt), goldensChanged: false }),
    [],
  );
  const refactor = manifest("1.0.1", { "profile.source_digest": { b: "3" } });
  assert.deepEqual(
    check({ base, head: refactor, changelog: entry(refactor), goldensChanged: false }),
    [],
  );
  assert.match(
    check({ base, head: refactor, changelog: entry(refactor), goldensChanged: true })[0],
    /replay goldens changed/,
  );
});

test("a missing changelog entry and a hand-edited id fail", () => {
  const m = manifest("1.0.0");
  assert.match(
    check({ base: null, head: m, changelog: "# Changelog\n", goldensChanged: false })[0],
    /no `## algo-1\.0\.0\+/,
  );
  const edited = { ...m, components: { ...m.components, profile: { detector_version: "tl-v2" } } };
  assert.match(
    check({ base: null, head: edited, changelog: entry(m), goldensChanged: false })[0],
    /does not match its content/,
  );
});

test("a step back is refused", () => {
  const base = manifest("2.0.0");
  const head = manifest("1.0.0");
  assert.match(
    check({ base, head, changelog: entry(head), goldensChanged: false })[0],
    /not a forward/,
  );
});

test("the committed manifest and changelog pass on their own", () => {
  const head = JSON.parse(readFileSync(join(root, MANIFEST), "utf8"));
  const changelog = readFileSync(join(root, CHANGELOG), "utf8");
  assert.deepEqual(check({ base: null, head, changelog, goldensChanged: false }), []);
});

const log = "# Changelog\n\n## algo-2.0.0+383bff27f04f\n\nText.\n\n## algo-1.0.0+fd96e117c013\n";

test("the changelog's versions are read from its entry headings", () => {
  assert.deepEqual(changelogVersions(log), ["2.0.0", "1.0.0"]);
  assert.deepEqual(changelogVersions("## algo-1.0.0\n### algo-2.0.0+ab12\n"), []);
});

test("MATH.md sections are `## algo-X.Y.Z`, with or without the hash", () => {
  assert.deepEqual(mathVersions("## algo-2.0.0\n## algo-1.0.0+fd96e117c013\n# algo-3.0.0\n"), [
    "2.0.0",
    "1.0.0",
  ]);
  assert.deepEqual(mathVersions("## algo-2.0.0 and more\n### algo-1.0.0\n"), []);
});

test("a version without its maths fails; a section for every version passes", () => {
  assert.deepEqual(missingMath(log, "# Maths\n\n## algo-2.0.0\n\n## algo-1.0.0\n"), []);
  const errors = missingMath(log, "# Maths\n\n## algo-1.0.0\n");
  assert.equal(errors.length, 1);
  assert.match(errors[0], /no `## algo-2\.0\.0` section/);
  assert.match(missingMath(log, null)[0], /MATH\.md does not exist/);
});

test("check reports missing maths when it is given MATH.md", () => {
  const m = manifest("1.0.0");
  const changelog = entry(m);
  assert.deepEqual(check({ base: null, head: m, changelog, goldensChanged: false }), []);
  assert.match(
    check({ base: null, head: m, changelog, goldensChanged: false, math: "# Maths\n" })[0],
    /no `## algo-1\.0\.0` section/,
  );
  assert.match(
    check({ base: null, head: m, changelog, goldensChanged: false, math: null })[0],
    /does not exist/,
  );
  assert.deepEqual(
    check({ base: null, head: m, changelog, goldensChanged: false, math: "## algo-1.0.0\n" }),
    [],
  );
});

test("the committed MATH.md has a section for every changelog version", () => {
  const changelog = readFileSync(join(root, CHANGELOG), "utf8");
  const math = readFileSync(join(root, MATH), "utf8");
  assert.ok(changelogVersions(changelog).length >= 10);
  assert.deepEqual(missingMath(changelog, math), []);
});

console.log(`${passed} passed`);
