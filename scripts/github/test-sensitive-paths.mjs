#!/usr/bin/env node
// Tests for the sensitive-paths matcher (ADR 0108), with fixtures of changed-file lists.
// Run: node scripts/github/test-sensitive-paths.mjs (no dependencies, no network, no git).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { matchSensitive, packageFieldChanges, showPath, summary } from "./sensitive-paths.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
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

const paths = (files, pkg) => matchSensitive(files, pkg).map((m) => m.path);

// Every path here must trip the check.
const SENSITIVE = [
  ".github/workflows/ci.yml",
  ".github/CODEOWNERS",
  ".github/dependabot.yml",
  ".github/settings/rulesets/main.json",
  ".github/PULL_REQUEST_TEMPLATE.md",
  "apps/web/Dockerfile",
  "backend/Dockerfile.dockerignore",
  "lab/Dockerfile",
  "docker-compose.yml",
  "docker-compose.e2e-prod.yml",
  "deploy/compose.yaml",
  "docker/postgres/init.sql",
  "pnpm-lock.yaml",
  "backend/Cargo.lock",
  "lab/uv.lock",
  "services/maia/requirements.lock",
  "pnpm-workspace.yaml",
  ".npmrc",
  "apps/web/.npmrc",
  "backend/Cargo.toml",
  "backend/crates/db/Cargo.toml",
  "backend/deny.toml",
  "backend/rust-toolchain.toml",
  "backend/.cargo/config.toml",
  "lab/pyproject.toml",
  ".gitattributes",
  "backend/crates/db/build.rs",
  "scripts/setup.sh",
  "scripts/github/sensitive-paths.mjs",
  "backend/scripts/check-crate-age.sh",
  "site/scripts/check-links.mjs",
  ".vscode/tasks.json",
  ".devcontainer/devcontainer.json",
  "CLAUDE.md",
  "packages/ui/CLAUDE.md",
  "AGENTS.md",
  ".claude/skills/design-system/SKILL.md",
  ".claude/settings.json",
  ".mcp.json",
  "backend/crates/ai/prompts/chat/system.v6.md",
  "services/ai-sidecar/src/options.ts",
  "services/ai-sidecar/src/local-env.ts",
  "algorithm/manifest.json",
  "algorithm/CHANGELOG.md",
  "SECURITY.md",
];

// None of these may trip it: ordinary product code, docs and tests.
const ORDINARY = [
  "apps/web/app/page.tsx",
  "apps/web/test/e2e/home.spec.ts",
  "packages/chess-core/src/fen.ts",
  "backend/crates/lichess-db/src/explorer_build.rs",
  "backend/crates/lichess-db/tests/explorer_build.rs",
  "backend/crates/ai/src/lib.rs",
  "services/ai-sidecar/src/server.ts",
  "docs/SDLC.md",
  "docs/decisions/0108-reviewing-outside-code.md",
  "README.md",
  "CONTRIBUTING.md",
  "site/src/content/docs/docs/contributing.md",
  "db/migrations/0042_add_index.sql",
  "packages/ui/DESIGN.md",
  "docs/scripts.md",
  "lab/rematch_lab/stats.py",
  "my-docker-compose-notes.md",
];

check("every fixture of a sensitive path trips the check", () => {
  const missed = SENSITIVE.filter((p) => paths([p]).length === 0);
  assert.deepEqual(missed, []);
});

check("ordinary code, docs and tests pass", () => {
  assert.deepEqual(paths(ORDINARY), []);
});

check("a mixed pull request lists only its sensitive files, in order, with reasons", () => {
  const m = matchSensitive(["apps/web/app/page.tsx", "pnpm-lock.yaml", "CLAUDE.md"]);
  assert.deepEqual(
    m.map((x) => x.path),
    ["pnpm-lock.yaml", "CLAUDE.md"],
  );
  assert.match(m[0].reasons[0], /lockfile/);
  assert.match(m[1].reasons[0], /prompt injection/);
});

check("a Windows-style path is normalised before matching", () => {
  assert.deepEqual(paths([".github\\workflows\\ci.yml"]), [".github/workflows/ci.yml"]);
});

check("package.json trips the check only when a risky field changed", () => {
  const before = JSON.stringify({ name: "a", version: "1.0.0", scripts: { test: "vitest" } });
  const bump = JSON.stringify({ name: "a", version: "1.0.1", scripts: { test: "vitest" } });
  const hook = JSON.stringify({
    name: "a",
    version: "1.0.0",
    scripts: { test: "vitest", postinstall: "node x.js" },
  });
  const dep = JSON.stringify({
    name: "a",
    version: "1.0.0",
    scripts: { test: "vitest" },
    dependencies: { "left-pad": "1.3.0" },
  });
  assert.deepEqual(packageFieldChanges(before, bump), []);
  assert.deepEqual(packageFieldChanges(before, hook), ["scripts"]);
  assert.deepEqual(packageFieldChanges(before, dep), ["dependencies"]);
  assert.deepEqual(packageFieldChanges(undefined, hook), ["scripts"]);
  assert.deepEqual(packageFieldChanges(hook, undefined), ["scripts"]);
  assert.deepEqual(packageFieldChanges(before, "{ not json"), ["(not valid JSON)"]);

  const pkg = new Map([
    ["apps/web/package.json", ["scripts"]],
    ["packages/ui/package.json", []],
  ]);
  const m = matchSensitive(["apps/web/package.json", "packages/ui/package.json"], pkg);
  assert.deepEqual(
    m.map((x) => x.path),
    ["apps/web/package.json"],
  );
  assert.equal(m[0].reasons[0], "package.json fields: scripts");
});

check("a hostile file name cannot start a workflow command or break a line", () => {
  const evil = "docs/a\n::error::fake\r.md";
  const shown = showPath(evil);
  assert.ok(!shown.includes("\n") && !shown.includes("\r"), shown);
  const text = summary(matchSensitive([".github/x\n::stop-commands::t"]));
  assert.ok(
    text.split("\n").every((l) => !l.startsWith("::")),
    text,
  );
});

check("the summary names the label and says when nothing matched", () => {
  assert.equal(summary([]), "No sensitive paths changed.");
  assert.match(summary(matchSensitive(["CLAUDE.md"])), /sensitive-reviewed/);
});

// The workflow itself: plain pull_request, a read-only token, pinned actions, the base copy of
// the matcher, and the owner exemption and the label override.
const wf = readFileSync(
  join(root, ".github", "workflows", "sensitive-paths.yml"),
  "utf8",
).replaceAll("\r\n", "\n");

check("sensitive-paths.yml: pull_request only, read-only, actions pinned by commit", () => {
  assert.ok(!wf.includes("pull_request_target"), "never pull_request_target");
  assert.match(wf, /^on:\n {2}pull_request:\n/m);
  assert.match(wf, /^permissions:\n {2}contents: read$/m);
  for (const line of wf.split("\n").filter((l) => /^\s*-?\s*uses:/.test(l))) {
    assert.match(line, /@[0-9a-f]{40}\b/, `not pinned by commit: ${line.trim()}`);
  }
  assert.match(wf, /persist-credentials: false/);
});

check(
  "sensitive-paths.yml: runs the base branch's matcher, exempts the owner, honours the label",
  () => {
    assert.match(wf, /ref: \$\{\{ github\.event\.pull_request\.base\.sha \}\}/);
    assert.match(wf, /node scripts\/github\/sensitive-paths\.mjs/);
    assert.match(wf, /AUTHOR: \$\{\{ github\.event\.pull_request\.user\.login \}\}/);
    assert.match(wf, /"\$AUTHOR" = "Terkea"/);
    assert.match(wf, /sensitive-reviewed/);
    // Untrusted values reach the shell only through env, never interpolated into a script.
    const lines = wf.split("\n");
    lines.forEach((line, i) => {
      const m = /^(\s*)run: (.*)$/.exec(line);
      if (!m) return;
      const script = [m[2]];
      for (const next of lines.slice(i + 1)) {
        if (next.trim() !== "" && next.search(/\S/) <= m[1].length) break;
        script.push(next);
      }
      const text = script.join("\n");
      assert.ok(!text.includes("${{"), `a run script interpolates an expression:\n${text}`);
    });
  },
);

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
