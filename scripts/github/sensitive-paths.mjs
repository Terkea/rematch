#!/usr/bin/env node
// The sensitive-paths check (ADR 0108). It lists the files a pull request changes that can run
// code on a maintainer's machine or in CI, change what ships, or steer an AI agent, so the
// maintainer reviews them by hand before anything of the pull request runs outside a container.
//
// It never runs pull request code: it reads `git diff` and `git show` output as data. The
// workflow runs the copy of this file from the base branch, so a pull request cannot weaken it.
//
//   node scripts/github/sensitive-paths.mjs --base <sha> --head <sha>
//
// prints the matches and exits 1 when there are any, 0 when there are none. Imported, it exports
// the rules and the matcher for scripts/github/test-sensitive-paths.mjs. No dependencies.
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const base = (p) => p.slice(p.lastIndexOf("/") + 1);

/**
 * Each rule names a reason and matches a repository-relative path (forward slashes). The order
 * is the order of the summary.
 */
export const RULES = [
  {
    id: "github",
    reason: "GitHub configuration: workflows, CODEOWNERS, Dependabot, settings as code",
    test: (p) => p.startsWith(".github/"),
  },
  {
    id: "containers",
    reason: "container images and deployment: what runs in CI and in production",
    test: (p) =>
      /^Dockerfile/.test(base(p)) ||
      /\.dockerignore$/.test(base(p)) ||
      /^docker-compose[^/]*\.ya?ml$/.test(base(p)) ||
      p.startsWith("deploy/") ||
      p.startsWith("docker/"),
  },
  {
    id: "lockfiles",
    reason: "lockfiles: every resolved dependency, with its source and checksum",
    test: (p) =>
      [
        "pnpm-lock.yaml",
        "package-lock.json",
        "yarn.lock",
        "bun.lockb",
        "Cargo.lock",
        "uv.lock",
      ].includes(base(p)) || /^requirements[^/]*\.lock$/.test(base(p)),
  },
  {
    id: "package-config",
    reason: "package manager and toolchain configuration: install hooks, registries, policy",
    test: (p) =>
      [
        ".npmrc",
        ".yarnrc",
        ".yarnrc.yml",
        "pnpm-workspace.yaml",
        ".pnpmfile.cjs",
        ".pnpmfile.mjs",
        "Cargo.toml",
        "deny.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
        "pyproject.toml",
        ".gitattributes",
        ".gitmodules",
      ].includes(base(p)) || /(^|\/)\.cargo\//.test(p),
  },
  {
    id: "build-scripts",
    reason: "build scripts and repository scripts: code that runs at build time or by hand",
    test: (p) => base(p) === "build.rs" || /(^|\/)scripts\//.test(p),
  },
  {
    id: "editor",
    reason: "editor and dev-container configuration: tasks that run when the folder opens",
    test: (p) =>
      p.startsWith(".vscode/") ||
      p.startsWith(".devcontainer/") ||
      p.startsWith(".husky/") ||
      /^lefthook[^/]*\.ya?ml$/.test(base(p)),
  },
  {
    id: "agent-instructions",
    reason: "instructions for AI agents (prompt injection into the agents that work on this repo)",
    test: (p) =>
      [
        "CLAUDE.md",
        "CLAUDE.local.md",
        "AGENTS.md",
        "GEMINI.md",
        ".cursorrules",
        ".mcp.json",
      ].includes(base(p)) ||
      /(^|\/)\.claude\//.test(p) ||
      /(^|\/)\.cursor\//.test(p),
  },
  {
    id: "ai-prompts",
    reason:
      "the coach's prompts and the AI sidecar's permissions (prompt injection into the coach)",
    test: (p) =>
      p.startsWith("backend/crates/ai/prompts/") ||
      /^services\/ai-sidecar\/src\/(options|local|local-env|tools|auth)\.ts$/.test(p),
  },
  {
    id: "algorithm",
    reason:
      "the algorithm manifest (ADR 0105) and the arena's gate: its rules, records and overrides (ADR 0109)",
    test: (p) =>
      p.startsWith("algorithm/") ||
      p === "lab/leaderboard.json" ||
      p === "lab/overrides.json" ||
      /^lab\/experiments\/EXP-\d{4}\/arena\.py$/.test(p),
  },
  {
    id: "security-policy",
    reason: "the security policy",
    test: (p) => p === "SECURITY.md",
  },
];

/** The package.json fields whose change can run code on install or change what is installed. */
export const PACKAGE_FIELDS = [
  "scripts",
  "pnpm",
  "packageManager",
  "bin",
  "workspaces",
  "dependencies",
  "devDependencies",
  "optionalDependencies",
  "peerDependencies",
  "overrides",
  "resolutions",
];

/**
 * The package.json fields that differ between two versions of the file (each `undefined` when
 * the file is absent on that side, or text that does not parse, which counts as every field).
 */
export function packageFieldChanges(before, after) {
  const parse = (text) => {
    if (text === undefined) return {};
    try {
      const v = JSON.parse(text);
      return v !== null && typeof v === "object" && !Array.isArray(v) ? v : null;
    } catch {
      return null;
    }
  };
  const a = parse(before);
  const b = parse(after);
  if (a === null || b === null) return ["(not valid JSON)"];
  return PACKAGE_FIELDS.filter((f) => JSON.stringify(a[f]) !== JSON.stringify(b[f]));
}

/**
 * The sensitive changes among `files` (repository-relative paths). `packageChanges` maps a
 * package.json path to its changed fields (packageFieldChanges). Returns [{path, reasons}].
 */
export function matchSensitive(files, packageChanges = new Map()) {
  const out = [];
  for (const raw of files) {
    const p = raw.replaceAll("\\", "/");
    const reasons = RULES.filter((r) => r.test(p)).map((r) => r.reason);
    if (base(p) === "package.json") {
      const fields = packageChanges.get(p) ?? [];
      if (fields.length > 0) reasons.push(`package.json fields: ${fields.join(", ")}`);
    }
    if (reasons.length > 0) out.push({ path: p, reasons });
  }
  return out;
}

/** A path as it is safe to print: control characters and quotes escaped (no workflow commands). */
export function showPath(p) {
  return JSON.stringify(p);
}

/** The text of the summary the check prints. */
export function summary(matches) {
  if (matches.length === 0) return "No sensitive paths changed.";
  const lines = [
    `This pull request changes ${matches.length} sensitive path(s). The maintainer reviews`,
    "each one by hand (docs/SDLC.md, the outside-code checklist), then adds the label",
    "`sensitive-reviewed`. New commits after that need the label removed and added again.",
    "",
  ];
  for (const m of matches) {
    lines.push(`- ${showPath(m.path)}`);
    for (const r of m.reasons) lines.push(`    ${r}`);
  }
  return lines.join("\n");
}

function git(args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

function showFile(rev, path) {
  try {
    return git(["show", `${rev}:${path}`]);
  } catch {
    return undefined;
  }
}

function main(argv) {
  const opt = (name) => {
    const i = argv.indexOf(name);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const baseRev = opt("--base");
  const headRev = opt("--head");
  if (!baseRev || !headRev) {
    console.error("usage: sensitive-paths.mjs --base <sha> --head <sha>");
    return 2;
  }
  // -z: names are NUL-separated and never quoted; --no-renames: a move shows both sides.
  const files = git(["diff", "--name-only", "--no-renames", "-z", `${baseRev}...${headRev}`])
    .split("\0")
    .filter(Boolean);
  const mergeBase = git(["merge-base", baseRev, headRev]).trim();
  const packageChanges = new Map();
  for (const f of files.filter((p) => base(p) === "package.json")) {
    packageChanges.set(f, packageFieldChanges(showFile(mergeBase, f), showFile(headRev, f)));
  }
  const matches = matchSensitive(files, packageChanges);
  const text = summary(matches);
  console.log(`${files.length} changed file(s).\n\n${text}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Sensitive paths\n\n${text}\n`);
  }
  return matches.length > 0 ? 1 : 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exit(main(process.argv.slice(2)));
}
