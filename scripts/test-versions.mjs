#!/usr/bin/env node
// One product version (ADR 0095): the root package.json is the source, and release-please keeps
// apps/web/package.json, the Cargo workspace and the workspace crates in Cargo.lock in step with
// it. This fails when they part ways, or when the release config drifts from the beta rules.
// Run: node scripts/test-versions.mjs (no dependencies, no network).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8").replaceAll("\r\n", "\n");
const json = (p) => JSON.parse(read(p));
const problems = [];
const expect = (ok, msg) => {
  if (!ok) problems.push(msg);
};

const version = json("package.json").version;
expect(
  /^\d+\.\d+\.\d+$/.test(version ?? ""),
  `root package.json version "${version}" is not x.y.z`,
);

expect(
  json("apps/web/package.json").version === version,
  `apps/web/package.json is ${json("apps/web/package.json").version}, the root is ${version}`,
);

const cargoToml = read("backend/Cargo.toml");
const ws =
  /^\[workspace\.package\]\n(?:.*\n)*?version = "([^"]+)" # x-release-please-version$/m.exec(
    cargoToml,
  );
expect(
  ws !== null,
  'backend/Cargo.toml: [workspace.package] needs version = "x.y.z" # x-release-please-version',
);
expect(
  ws?.[1] === version,
  `backend/Cargo.toml workspace version is ${ws?.[1]}, the root is ${version}`,
);

// Workspace crates are the Cargo.lock packages without a `source` (the registry ones have one).
const lock = read("backend/Cargo.lock");
const local = lock
  .split("\n[[package]]\n")
  .slice(1)
  .filter((p) => !/^source = /m.test(p))
  .map((p) => ({
    name: /^name = "([^"]+)"/m.exec(p)?.[1],
    version: /^version = "([^"]+)"/m.exec(p)?.[1],
  }));
expect(local.length > 0, "backend/Cargo.lock has no workspace crates");
for (const c of local) {
  expect(
    c.version === version,
    `backend/Cargo.lock: ${c.name} is ${c.version}, the root is ${version}`,
  );
}

// The release config: beta rules while 0.y.z, the three extra files, and the first release.
const config = json("release-please-config.json");
const pkg = config.packages?.["."];
expect(pkg !== undefined && Object.keys(config.packages).length === 1, "one package, at the root");
expect(pkg?.["release-type"] === "node", "release-type is node");
expect(pkg?.["bump-minor-pre-major"] === true, "a breaking change bumps the minor while 0.y.z");
expect(pkg?.["bump-patch-for-minor-pre-major"] === true, "feat bumps the patch while 0.y.z");
const major = Number(version.split(".")[0]);
expect(pkg?.prerelease === major < 1, `prerelease must be ${major < 1} at version ${version}`);
const extra = (pkg?.["extra-files"] ?? []).map((f) => f.path).sort();
expect(
  JSON.stringify(extra) ===
    JSON.stringify(["apps/web/package.json", "backend/Cargo.lock", "backend/Cargo.toml"]),
  `extra-files are ${extra}`,
);

const released = json(".release-please-manifest.json")["."];
if (released === "0.0.0") {
  // Nothing released yet: the first release PR proposes exactly the root version.
  expect(pkg?.["release-as"] === version, `first release: release-as must be ${version}`);
} else {
  expect(
    released === version,
    `.release-please-manifest.json is ${released}, the root is ${version}`,
  );
  if (pkg?.["release-as"] === released) {
    console.log(
      `::warning::release-as ${released} is released; remove it from release-please-config.json`,
    );
  }
}

if (problems.length > 0) {
  for (const p of problems) console.log(`FAIL ${p}`);
  process.exit(1);
}
console.log(`ok   one version everywhere: ${version} (${local.length} workspace crates)`);
