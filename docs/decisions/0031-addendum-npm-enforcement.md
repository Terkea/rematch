# 0031 addendum. npm enforcement

Date: 2026-11-21
Status: Accepted. Amends 0031 (Rust workspace, toolchain and crates; lives on the r1 branches).

## Context

ADR 0031 and CLAUDE.md require every package to be at least 7 days old and pinned exactly. For crates, `backend/scripts/check-crate-age.sh` enforces it. For npm nothing did: `minimumReleaseAge` was unset, so pnpm 12 used its built-in default (1440 minutes, non-strict) and ranges were accepted.

## Decision

- `pnpm-workspace.yaml` sets `minimumReleaseAge: 10080` (7 days, in minutes) and `minimumReleaseAgeStrict: true`. pnpm then also verifies the lockfile against the policy on install. We never use `minimumReleaseAgeExclude` to get around it.
- `scripts/check-npm-age.mjs` is the npm counterpart of the crate script. It reads every resolved version in `pnpm-lock.yaml`, fetches the publish time from the npm registry (cached in the git-ignored `.cache/npm-age/`), and fails on any version younger than 7 days. It also fails if a direct dependency in any workspace `package.json` is a range (`^`, `~`, `*`, `>=`, ...). `workspace:` and `catalog:` specifiers are allowed.
- CI runs the script before `pnpm install`.
- All direct dependencies were pinned to the version already in the lockfile; no resolved version changed.

## Consequences

Versions that are too young cannot enter the lockfile, and a range cannot be committed. When a package is too young, wait or pin an older, aged version; do not exclude it.
