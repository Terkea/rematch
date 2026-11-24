# Security policy

Report security problems privately, through GitHub. Many thanks for doing it that way.

## Reporting a vulnerability

Don't open a public issue for a security problem. Report it privately:

1. Go to the repository's [Security tab](https://github.com/Terkea/rematch/security).
2. Choose **Report a vulnerability** ([direct link](https://github.com/Terkea/rematch/security/advisories/new)).
3. Describe the problem, how to reproduce it, and what an attacker could do with it.

Only the maintainer can read the report. No email address is published; GitHub private
vulnerability reporting is the one channel.

What to expect:

- an acknowledgement within 7 days;
- an assessment and a plan, or questions, within 14 days;
- a fix released as soon as it is ready, with credit to you in the advisory if you want it.

Please give us a reasonable time to fix the problem before you disclose it, and don't access,
change or delete other people's data while testing. Testing against your own local stack is
always fine.

## Supported versions

Rematch is in beta (`0.y.z`).

| Version | Supported |
| --- | --- |
| latest `0.y.z` release | yes |
| older releases | no, please upgrade |

## Scope

In scope: this repository's code, its Docker images and compose files, and the CI workflows.

Out of scope: chess.com, Lichess, Stockfish, Anthropic and other third-party services and
software (report those to their owners), and findings that need a compromised host.

## How the project protects itself

- `main` is protected by a ruleset kept as code in `.github/settings/`: pull requests only,
  required CI checks, no force pushes. Only the maintainer merges.
- CI uses no secrets, and fork pull requests run with a read-only token.
- Dependencies are pinned exactly, must be at least 7 days old, and Rust crates are checked by
  `cargo deny` for licences and advisories.

## How we review outside code

Every pull request from outside the project is treated as untrusted until the maintainer has read
it (ADR 0108).

- Fork pull requests wait for the maintainer's approval before any workflow runs, for every outside
  contributor, not only first-timers (`.github/settings/repo.json`, `actions`).
- Workflows run on `pull_request`, never `pull_request_target`, with a read-only `GITHUB_TOKEN`
  and no secrets, so a fork's code in CI cannot push, release or read a credential.
- The `Sensitive paths` check flags a change to a path where outside code can run on a
  maintainer's machine or in CI, change what ships, or steer an AI agent:
  - `.github/` (workflows, CODEOWNERS, Dependabot, the settings as code);
  - Dockerfiles, `docker-compose*.yml`, `deploy/` and `docker/`;
  - lockfiles (`pnpm-lock.yaml`, `Cargo.lock`, `uv.lock`), `package.json` scripts and dependency
    lists, `pnpm-workspace.yaml`, `.npmrc`, `Cargo.toml`, `deny.toml`, `pyproject.toml`;
  - `build.rs`, every `scripts/` directory, `.vscode/` and `.devcontainer/`;
  - `CLAUDE.md`, `AGENTS.md`, `.claude/` and `.mcp.json`;
  - the coach's prompts (`backend/crates/ai/prompts/`) and the AI sidecar's permission files;
  - `algorithm/` and this file.

  The maintainer reviews those files by hand and only then adds the `sensitive-reviewed` label.
  The full list is `scripts/github/sensitive-paths.mjs`, and CODEOWNERS names each path.
- `CLAUDE.md`, `.claude/` and the prompts count because AI agents build much of Rematch and read
  them as instructions. A line added there is prompt injection: it runs with the agent's
  permissions on the maintainer's machine, or with the coach's in production.
- Maintainers never run pull request code outside an isolated container. Diffs are read with
  `gh pr diff`; a branch is built and tested only inside the compose containers, never with
  `pnpm install`, `cargo build` or a script on the host.
