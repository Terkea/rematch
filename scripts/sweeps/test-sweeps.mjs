#!/usr/bin/env node
// Tests for the sweep checks (ADR 0099). Run: node scripts/sweeps/test-sweeps.mjs (no network).
import assert from "node:assert/strict";
import { adrIdOf, adrIndexProblems, adrReferences, advisoryIgnores } from "./sweeps-lib.mjs";

let failed = 0;
let passed = 0;
function check(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok   ${name}`);
  } catch (err) {
    failed += 1;
    console.log(`FAIL ${name}\n     ${String(err.message).split("\n").join("\n     ")}`);
  }
}

const deny = (entries) =>
  `[licenses]\nallow = ["MIT"]\n\n[advisories]\nversion = 2\nignore = [\n${entries}\n]\n\n[bans]\nmultiple-versions = "warn"\n`;

check("advisoryIgnores: a dated ignore passes until its day and fails the day after", () => {
  const toml = deny(`  { id = "RUSTSEC-2026-0592", reason = "x" }, # expires: 2026-11-27`);
  assert.deepEqual(advisoryIgnores(toml, "2026-11-27"), {
    entries: [{ id: "RUSTSEC-2026-0592", expires: "2026-11-27" }],
    problems: [],
  });
  assert.match(advisoryIgnores(toml, "2026-11-28").problems.join(), /expired on 2026-11-27/);
});

check("advisoryIgnores: an ignore without an expiry fails, an empty list passes", () => {
  const toml = deny(`  { id = "RUSTSEC-2026-0001", reason = "x" },`);
  assert.match(advisoryIgnores(toml, "2026-01-01").problems.join(), /no "# expires/);
  assert.deepEqual(advisoryIgnores(deny(""), "2026-01-01").problems, []);
  assert.deepEqual(advisoryIgnores("[advisories]\nignore = []\n", "2026-01-01").entries, []);
});

check("advisoryIgnores: ids outside [advisories] are not ignores", () => {
  const toml = `[bans]\nskip = [{ id = "RUSTSEC-0000-0000" }]\n${deny("")}`;
  assert.deepEqual(advisoryIgnores(toml, "2026-01-01").entries, []);
});

check("adrReferences: numbered, listed and lettered ids", () => {
  const text =
    "See ADR 0095, ADRs 0030, 0031 and 0042, ADR-R2-12, ADR-WEB-7 and ADR-FIX-N-1. Not 2026.";
  assert.deepEqual(adrReferences(text).sort(), [
    "0030",
    "0031",
    "0042",
    "0095",
    "ADR-FIX-N-1",
    "ADR-R2-12",
    "ADR-WEB-7",
  ]);
});

check("adrIdOf: numbered and lettered files", () => {
  assert.equal(adrIdOf("0095-open-source.md"), "0095");
  assert.equal(adrIdOf("ADR-R2-12-syzygy.md"), "ADR-R2-12");
  assert.equal(adrIdOf("ADR-FIX-N-1-links.md"), "ADR-FIX-N-1");
  assert.equal(adrIdOf("README.md"), null);
});

check("adrIndexProblems: unlisted files and dead links", () => {
  const readme = "- [0001. A](0001-a.md)\n- [0003. C](0003-c.md)\n";
  assert.deepEqual(adrIndexProblems(readme, ["README.md", "0001-a.md", "0002-b.md"]), [
    "docs/decisions/README.md links 0003-c.md, which does not exist",
    "docs/decisions/0002-b.md is not listed in docs/decisions/README.md",
  ]);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
