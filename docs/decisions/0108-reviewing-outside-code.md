# 0108. Reviewing outside code: sensitive paths, fork approval and a read-only token

Date: 2026-11-24. Status: accepted (lane `prsec`, branch `ci/contribution-security`). Builds on ADR 0095.

## Context

The repository is public, and outside pull requests have started to arrive (#85, and the Dependabot updates #1 to #9). The owner asked us to "be aware of code contributors, they could be injecting malware or what not". A pull request can attack the project in three places:

- CI. A `pull_request` workflow runs the pull request's own workflow files and scripts. With a read-only token and no secrets that run can waste minutes, but it cannot push, release or read a secret.
- The maintainer's machine. A changed `package.json` script, a `postinstall` hook in a new dependency, a `build.rs`, a `docker-compose` mount or a `.vscode/tasks.json` runs as soon as the maintainer installs, builds or opens the branch.
- The AI agents. Agents build most of Rematch, and they read `CLAUDE.md`, `.claude/` and `.mcp.json` as instructions. A sentence added there is prompt injection with the agent's permissions. The coach's prompts (`backend/crates/ai/prompts/`) and the sidecar's permission options do the same to the coach in production.

## Decision

1. **The sensitive-paths check.** `.github/workflows/sensitive-paths.yml` fails when a pull request changes `.github/`, a Dockerfile, a compose file, `deploy/` or `docker/`, a lockfile, package-manager or toolchain configuration (`pnpm-workspace.yaml`, `.npmrc`, `Cargo.toml`, `deny.toml`, `pyproject.toml`, `.cargo/`), the risky fields of a `package.json` (`scripts`, `pnpm`, `packageManager`, `bin` and the dependency lists), a `build.rs`, any `scripts/` directory, editor auto-run configuration, `CLAUDE.md`, `AGENTS.md`, `.claude/`, `.mcp.json`, the coach's prompts and the sidecar's permission files, `algorithm/`, or `SECURITY.md`. The rules and their reasons are in `scripts/github/sensitive-paths.mjs`, tested by `scripts/github/test-sensitive-paths.mjs` with fixtures of changed-file lists.
2. **It runs no pull request code.** It runs on `pull_request`, never `pull_request_target`, with `contents: read`, the checkout action pinned by commit and `persist-credentials: false`. It checks out the base commit, fetches the pull request's commits as data, and runs the base copy of the matcher over `git diff --name-only --no-renames -z`. A pull request therefore cannot weaken the matcher; it can edit the workflow file, which is why `.github/` is itself a sensitive path and why the check is advisory (point 4).
3. **The override is a label.** The maintainer reviews the flagged files by hand (the checklist in `docs/SDLC.md`) and adds `sensitive-reviewed`. Labels need triage rights, so a contributor cannot set it. The label counts only on the run its own `labeled` event starts, so a commit pushed after the review fails the check again until the label is removed and added back. Pull requests opened by the owner, `Terkea`, are exempt. Dependabot's lockfile and workflow updates trip the check on purpose.
4. **Advisory, not required.** The check is visible on every pull request but is not in the ruleset's required checks. A required check that the pull request can rewrite gives false assurance: a hostile `sensitive-paths.yml` could simply exit 0. The real gates are the ones a pull request cannot touch: fork approval (point 5), CODEOWNERS, and the maintainer as the only person who merges. Making it required would also put a failing check on every Dependabot pull request until labelled, with nothing gained, since the maintainer's admin bypass skips required checks anyway.
5. **Actions settings as code.** `.github/settings/repo.json` gains `actions`: `fork_pr_contributor_approval: all_external_contributors` (every outside contributor's workflow run waits for the maintainer's approval, not only a first-timer's), and `default_workflow_permissions: read` with `can_approve_pull_request_reviews: false`. `scripts/github/apply-settings.sh` reads `actions/permissions/fork-pr-contributor-approval` and `actions/permissions/workflow` and writes only what differs. The dry run on 2026-11-24 found the token already read-only and the approval policy at `first_time_contributors`.
6. **CODEOWNERS names every sensitive path** explicitly, so a later, narrower rule cannot hand one to someone else.
7. **The maintainer never runs pull request code outside an isolated container.** Diffs are read with `gh pr diff`; a branch is built only inside the compose containers with no host mounts beyond the checkout, never with `pnpm install` on the host.

## Options considered

1. **`pull_request_target` with a trusted workflow** (recommended against). It would run the base copy of the workflow and make the check tamper-proof, but it runs with a write token and secrets in the base repository's context. One later edit that checks out the head ref turns it into the best-known way to steal a repository's secrets. ADR 0095 rules it out and so do we.
2. **A required check** (rejected, point 4).
3. **An advisory check plus fork approval and CODEOWNERS** (chosen).

## Consequences

- Each outside pull request needs one approval click before its workflows run, and one label before the sensitive-paths check turns green.
- `.github/settings/labels.json` gains `sensitive-reviewed`. Until `apply-settings.sh` runs after the merge, the settings drift check reports that label and the approval policy as drift.
- A new kind of sensitive file needs a rule in `sensitive-paths.mjs`, a fixture in its test and a line in CODEOWNERS.
