# 0031 addendum. npm enforcement

Date: 2026-11-21
Status: Accepted. Amends 0031 (Rust workspace, toolchain and crates; lives on the r1 branches).

## Context

ADR 0031 and CLAUDE.md require every package to be at least 7 days old and pinned exactly. For crates, `backend/scripts/check-crate-age.sh` enforces it. For npm nothing did: `minimumReleaseAge` was unset, so pnpm 12 used its built-in default (1440 minutes, non-strict) and ranges were accepted.

## Decision

- `pnpm-workspace.yaml` sets `minimumReleaseAge: 10080` (7 days, in minutes) and `minimumReleaseAgeStrict: true`. pnpm then also verifies the lockfile against the policy on install. We never use `minimumReleaseAgeExclude` to get around it.
- `scripts/check-npm-age.mjs` is the npm counterpart of the crate script. It reads every resolved version in `pnpm-lock.yaml`, fetches the publish time from the npm registry (cached in the git-ignored `.cache/npm-age/`), and fails on any version younger than 7 days. It also fails if a direct dependency in any workspace `package.json` is a range (`^`, `~`, `*`, `>=`, ...). `workspace:` and `catalog:` specifiers are allowed.
- CI runs the script before `pnpm install`.
- All direct dependencies are pinned exactly. Versions that failed the age check were downgraded to the newest stable release at least 7 days old, and the lockfile was regenerated under the strict policy (no excludes, no overrides).
- `@playwright/mcp` is not a dependency of the workspace. Every published version depends on an exact alpha `playwright`, so keeping it in `package.json` would put a prerelease in the lockfile. The Playwright MCP runs only through the pinned `npx` entry in `.mcp.json`, as dev tooling that is never shipped, and that entry must pin an exact version that is at least 7 days old (never `@latest`).
- Known residual: the lockfile still contains `@emnapi/core` and `@emnapi/runtime` 2.0.0-beta.2. They are auto-installed peers of `@napi-rs/wasm-runtime`, used only by rolldown's optional wasm32 binding, which never installs on a native platform. They pass the age check.

## Consequences

Versions that are too young cannot enter the lockfile, and a range cannot be committed. When a package is too young, wait or pin an older, aged version; do not exclude it.
