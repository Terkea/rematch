#!/usr/bin/env node
// Tests of scripts/lab/pypi-age-lib.mjs (the PyPI age rule, ADR 0106). No network, no
// dependencies: node scripts/lab/test-pypi-age.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ageViolations,
  excludeNewer,
  firstUpload,
  foreignSources,
  parseUvLock,
  pinViolations,
  pyprojectRequirements,
} from "./pypi-age-lib.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok ${n} ${name}`);
};

const LOCK = `version = 1
[options]
exclude-newer = "2026-11-16T00:00:00Z"

[[package]]
name = "numpy"
version = "2.5.3"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "x", hash = "sha256:a", size = 1, upload-time = "2026-09-06T10:00:00Z" }
wheels = [
    { url = "y", hash = "sha256:b", size = 1, upload-time = "2026-09-06T09:00:00Z" },
]

[[package]]
name = "rematch-lab"
version = "0.1.0"
source = { virtual = "." }

[[package]]
name = "evil"
version = "1.0"
source = { git = "https://example.com/evil.git" }
`;

test("parses the lock's packages, sources and upload times", () => {
  const p = parseUvLock(LOCK);
  assert.equal(p.length, 3);
  assert.deepEqual(p[0], {
    name: "numpy",
    version: "2.5.3",
    sourceKind: "registry",
    source: "https://pypi.org/simple",
    uploads: ["2026-09-06T10:00:00Z", "2026-09-06T09:00:00Z"],
  });
  assert.equal(p[1].sourceKind, "virtual");
});

test("reads the exclude-newer cut-off", () => {
  assert.equal(excludeNewer(LOCK), "2026-11-16T00:00:00Z");
  assert.equal(excludeNewer("version = 1\n"), null);
});

test("flags a package from anywhere but PyPI, never the project itself", () => {
  assert.deepEqual(foreignSources(parseUvLock(LOCK)), [
    "evil@1.0 from git https://example.com/evil.git",
  ]);
});

test("the age rule: under 7 days fails, 7 or more passes, no time is missing", () => {
  const pkgs = [
    { name: "a", version: "1" },
    { name: "b", version: "1" },
    { name: "c", version: "1" },
  ];
  const now = Date.parse("2026-11-23T00:00:00Z");
  const times = new Map([
    ["a@1", "2026-11-20T00:00:00Z"],
    ["b@1", "2026-11-16T00:00:00Z"],
  ]);
  const { young, missing } = ageViolations(pkgs, times, now);
  assert.equal(young.length, 1);
  assert.match(young[0], /^a@1 published .* \(3\.0 days old\)$/);
  assert.deepEqual(missing, ["c@1"]);
});

test("the earliest file of a release is its publish time", () => {
  assert.equal(
    firstUpload({
      urls: [
        { upload_time_iso_8601: "2026-09-06T10:00:00Z" },
        { upload_time_iso_8601: "2026-09-06T09:00:00Z" },
      ],
    }),
    "2026-09-06T09:00:00Z",
  );
  assert.equal(firstUpload({ urls: [] }), null);
});

test("only exact == pins pass", () => {
  assert.deepEqual(
    pinViolations(["numpy==2.5.3", "arviz[all]==1.3.0", "scipy>=1.18", "pandas", "x~=1.0"]),
    ["scipy>=1.18", "pandas", "x~=1.0"],
  );
});

test("reads project dependencies and dependency groups", () => {
  const reqs = pyprojectRequirements(`[project]
name = "x"
dependencies = [
    "numpy==2.5.3",
    "scipy==1.18.1",
]

[dependency-groups]
dev = ["pytest==9.2.0"]

[tool.uv]
package = false
`);
  assert.deepEqual(reqs, ["numpy==2.5.3", "scipy==1.18.1", "pytest==9.2.0"]);
});

test("the committed lab is pinned exactly, PyPI only, with a cut-off", () => {
  const lock = readFileSync(join(ROOT, "lab", "uv.lock"), "utf8");
  const reqs = pyprojectRequirements(readFileSync(join(ROOT, "lab", "pyproject.toml"), "utf8"));
  assert.ok(reqs.length >= 10, `found ${reqs.length} requirements`);
  assert.deepEqual(pinViolations(reqs), []);
  assert.deepEqual(foreignSources(parseUvLock(lock)), []);
  assert.ok(excludeNewer(lock));
  for (const r of reqs) {
    const [name, version] = r.split("==");
    assert.ok(
      parseUvLock(lock).some((p) => p.name === name && p.version === version),
      `${r} is not what the lock resolved`,
    );
  }
});

console.log(`${n} tests passed`);
