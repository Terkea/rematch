#!/usr/bin/env node
// Tests for the release pipeline's logic (ADR 0098): image tags, the CI gate on the tagged commit,
// the released compose file, the release-notes header and the GPL source manifest.
// Run: node scripts/release/test-release.mjs (no dependencies, no network).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ciGateVerdict,
  composeImageRefs,
  IMAGES,
  imageTags,
  mergeReleaseNotes,
  parseSourceManifest,
  parseTag,
  REGISTRY,
  renderReleaseCompose,
  renderTemplate,
  requiredCiChecks,
} from "./release-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (p) => readFileSync(join(root, p), "utf8").replaceAll("\r\n", "\n");
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

check("parseTag: v-prefixed SemVer only", () => {
  assert.deepEqual(parseTag("v0.1.0"), { version: "0.1.0", major: 0, minor: 1, patch: 0, pre: "" });
  assert.equal(parseTag("v1.2.3-rc.1").pre, "rc.1");
  assert.throws(() => parseTag("0.1.0"), /v/);
  assert.throws(() => parseTag("v0.1"), /SemVer/);
  assert.throws(() => parseTag("refs/tags/v0.1.0"), /v/);
});

check("imageTags: X.Y.Z, X.Y and beta below 1.0, never latest", () => {
  assert.deepEqual(imageTags("v0.1.0"), ["0.1.0", "0.1", "beta"]);
  assert.deepEqual(imageTags("v0.12.3"), ["0.12.3", "0.12", "beta"]);
  assert.ok(!imageTags("v0.9.9").includes("latest"));
});

check("imageTags: from 1.0, X.Y.Z, X.Y, X and latest; a prerelease gets only its own tag", () => {
  assert.deepEqual(imageTags("v1.0.0"), ["1.0.0", "1.0", "1", "latest"]);
  assert.deepEqual(imageTags("v1.1.0-rc.1"), ["1.1.0-rc.1"]);
  assert.deepEqual(imageTags("v0.2.0-rc.1"), ["0.2.0-rc.1"]);
});

check("IMAGES: web, worker and ai-sidecar, each a Dockerfile target that exists", () => {
  assert.deepEqual(
    IMAGES.map((i) => i.name),
    ["web", "worker", "ai-sidecar"],
  );
  for (const image of IMAGES) {
    const dockerfile = read(image.dockerfile);
    assert.match(dockerfile, new RegExp(`^FROM .+ AS ${image.target}$`, "m"), image.dockerfile);
    assert.equal(image.repository, `${REGISTRY}/rematch-${image.name}`);
  }
});

check("every published image ships the NOTICE and the AGPL text", () => {
  for (const image of IMAGES) {
    const stage = read(image.dockerfile).split(new RegExp(`^FROM .+ AS ${image.target}$`, "m"))[1];
    assert.ok(stage, image.dockerfile);
    assert.match(
      stage.split(/^FROM /m)[0],
      /^COPY (--chown=\S+ )?LICENSE NOTICE \/usr\/share\/doc\/rematch\/$/m,
      `${image.dockerfile} (${image.target}) must COPY LICENSE NOTICE /usr/share/doc/rematch/`,
    );
  }
  assert.match(read("backend/Dockerfile.dockerignore"), /^!LICENSE$/m);
  assert.match(read("backend/Dockerfile.dockerignore"), /^!NOTICE$/m);
});

check("the worker image keeps Stockfish's licence and authors next to the binary", () => {
  const dockerfile = read("backend/Dockerfile");
  assert.match(dockerfile, /install -D -m 0644 \/tmp\/stockfish\/Copying\.txt/);
  const worker = dockerfile.split(/^FROM .+ AS worker$/m)[1];
  assert.match(
    worker,
    /^COPY --from=stockfish \/usr\/share\/doc\/stockfish \/usr\/share\/doc\/stockfish$/m,
  );
});

check("requiredCiChecks: the ruleset's checks that ci.yml reports on a push to main", () => {
  const ci = "jobs:\n  ts:\n    runs-on: x\n  rust:\n    runs-on: x\n";
  const ruleset = {
    rules: [
      {
        type: "required_status_checks",
        parameters: {
          required_status_checks: [{ context: "ts" }, { context: "rust" }, { context: "pr-title" }],
        },
      },
    ],
  };
  assert.deepEqual(requiredCiChecks(ruleset, ci), ["rust", "ts"]);
  // The real files: pr-title only runs on pull requests, so it is never waited for.
  const real = requiredCiChecks(
    JSON.parse(read(".github/settings/rulesets/main.json")),
    read(".github/workflows/ci.yml"),
  );
  assert.ok(real.includes("ts") && real.includes("rust") && real.includes("e2e"), `${real}`);
  assert.ok(!real.includes("pr-title"));
});

check("ciGateVerdict: pass only when every required check's newest run succeeded", () => {
  const run = (name, status, conclusion, started) => ({
    name,
    status,
    conclusion,
    started_at: started ?? "2026-11-23T10:00:00Z",
  });
  const req = ["ts", "rust"];
  assert.equal(
    ciGateVerdict(req, [run("ts", "completed", "success"), run("rust", "completed", "success")])
      .state,
    "pass",
  );
  const waiting = ciGateVerdict(req, [run("ts", "completed", "success")]);
  assert.equal(waiting.state, "wait");
  assert.match(waiting.detail, /rust: not started/);
  assert.equal(
    ciGateVerdict(req, [run("ts", "in_progress", null), run("rust", "completed", "success")]).state,
    "wait",
  );
  const failedRun = ciGateVerdict(req, [
    run("ts", "completed", "failure"),
    run("rust", "completed", "success"),
  ]);
  assert.equal(failedRun.state, "fail");
  assert.match(failedRun.detail, /ts: failure/);
  // A cancelled run (a newer push cancelled it) fails the gate: re-run CI on that commit.
  assert.equal(
    ciGateVerdict(req, [run("ts", "completed", "cancelled"), run("rust", "completed", "success")])
      .state,
    "fail",
  );
  // A re-run that passed supersedes the earlier failure.
  assert.equal(
    ciGateVerdict(req, [
      run("ts", "completed", "failure", "2026-11-23T10:00:00Z"),
      run("ts", "completed", "success", "2026-11-23T11:00:00Z"),
      run("rust", "completed", "success"),
    ]).state,
    "pass",
  );
  // Skipped is not green.
  assert.equal(
    ciGateVerdict(req, [run("ts", "completed", "skipped"), run("rust", "completed", "success")])
      .state,
    "fail",
  );
});

const compose = read("deploy/compose.yaml");
const version = JSON.parse(read("package.json")).version;

check(
  "deploy/compose.yaml: every published image, at the product version, marked for release-please",
  () => {
    const refs = composeImageRefs(compose);
    assert.deepEqual(refs.map((r) => r.image).sort(), IMAGES.map((i) => i.name).sort());
    for (const r of refs) {
      assert.equal(r.version, version, `${r.image} is ${r.version}, the root is ${version}`);
      assert.ok(r.marked, `${r.image}'s line needs # x-release-please-version`);
    }
    assert.ok(!/\bbuild:/.test(compose), "the release file pulls images, it never builds");
    assert.ok(!/:latest\b/.test(compose), "no latest tag");
  },
);

check(
  "deploy/compose.yaml: Postgres and Redis pinned by digest, Postgres with init and exec healthcheck",
  () => {
    assert.match(compose, /image: postgres:17@sha256:[0-9a-f]{64}$/m);
    assert.match(compose, /image: redis:7@sha256:[0-9a-f]{64}$/m);
    assert.match(compose, /^ {4}init: true$/m);
    assert.match(compose, /test: \["CMD", "pg_isready", "-h", "127\.0\.0\.1"/);
    assert.match(compose, /BETTER_AUTH_SECRET: \$\{BETTER_AUTH_SECRET:\?/);
    assert.match(compose, /POSTGRES_PASSWORD: \$\{POSTGRES_PASSWORD:\?/);
  },
);

check("renderReleaseCompose: pins each image to its pushed digest and drops the overrides", () => {
  const digests = {
    web: `sha256:${"a".repeat(64)}`,
    worker: `sha256:${"b".repeat(64)}`,
    "ai-sidecar": `sha256:${"c".repeat(64)}`,
  };
  const out = renderReleaseCompose(compose, { version, digests });
  for (const image of IMAGES) {
    assert.ok(
      out.includes(`image: ${image.repository}:${version}@${digests[image.name]}`),
      `${image.name} not pinned`,
    );
  }
  assert.ok(!out.includes("${REMATCH_IMAGE_PREFIX"), "the smoke-test override is gone");
  assert.ok(!out.includes("${REMATCH_VERSION"), "the version override is gone");
  assert.ok(!out.includes("x-release-please-version"), "the marker is gone");
  assert.throws(
    () => renderReleaseCompose(compose, { version, digests: { web: digests.web } }),
    /worker/,
  );
  assert.throws(
    () => renderReleaseCompose(compose, { version, digests: { ...digests, web: "sha256:12" } }),
    /digest/,
  );
});

check("mergeReleaseNotes: the header goes on top once, release-please's changelog stays", () => {
  const header = "<!-- rematch-header -->\nBeta.\n<!-- /rematch-header -->";
  const body = "## [0.1.0](x) (2026-11-24)\n\n### Features\n\n* a thing";
  const once = mergeReleaseNotes(body, header);
  assert.ok(once.startsWith(header));
  assert.ok(once.endsWith(body));
  assert.equal(mergeReleaseNotes(once, header), once, "idempotent on a re-run");
  const newer = header.replace("Beta.", "Beta, again.");
  assert.equal(mergeReleaseNotes(once, newer), `${newer}\n\n${body}`);
});

check("the release-notes template renders with every placeholder filled", () => {
  const template = read(".github/release-notes/header.md");
  const out = renderTemplate(template, {
    version: "0.1.0",
    tag: "v0.1.0",
    repo: "Terkea/rematch",
    status: "beta",
  });
  assert.ok(!/\{\{/.test(out), out);
  assert.match(out, /cosign verify/);
  assert.match(out, /gh attestation verify/);
  assert.match(out, /ghcr\.io\/terkea\/rematch-web:0\.1\.0/);
  assert.match(out, /^<!-- rematch-header -->$/m);
  assert.throws(() => renderTemplate("{{nope}}", {}), /nope/);
});

check("deploy/stockfish-source.txt: the sf_19 source and its network, pinned by SHA-256", () => {
  const entries = parseSourceManifest(read("deploy/stockfish-source.txt"));
  assert.deepEqual(
    entries.map((e) => e.file),
    ["stockfish-sf_19-source.tar.gz", "nn-1a298aa575a0.nnue"],
  );
  for (const e of entries) {
    assert.match(e.sha256, /^[0-9a-f]{64}$/);
    assert.match(e.url, /^https:\/\//);
  }
  // The network file name is the first 12 hex digits of its SHA-256 (Stockfish's own rule).
  const net = entries[1];
  assert.equal(net.file, `nn-${net.sha256.slice(0, 12)}.nnue`);
  // The binary the images run comes from the same release tag as the source.
  assert.match(read("backend/Dockerfile"), /releases\/download\/sf_19\//);
  assert.match(entries[0].url, /\/refs\/tags\/sf_19\.tar\.gz$/);
  assert.throws(() => parseSourceManifest("abc  file  https://x"), /SHA-256/);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
