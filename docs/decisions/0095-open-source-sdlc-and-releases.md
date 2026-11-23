# 0095. Open source under AGPL-3.0, with the SDLC, protection and releases as code

- Status: accepted
- Date: 2026-11-23
- Lane: L1 (`chore/open-source-foundation`)

## Context

The project goes public as **Rematch, chess improver** so the community can help build it. A
public repository needs a licence, community files, a clear way for strangers to contribute, a
`main` branch only the maintainer can change, and releases people can follow. The repository is
private today (free plan), where rulesets are not available; they start working the moment it
is public. The owner decided (2026-11-23): AGPL-3.0, only the owner merges to `main`, the rules
kept as code and applied from the owner's login with no stored admin token, the history kept as
it is, and a beta versioned from 0 until there is an MVP.

## Decision

1. **Licence: `AGPL-3.0-only`.** `LICENSE` is GitHub's AGPL-3.0 text; every `package.json` and the
   Cargo workspace carry the SPDX id. `NOTICE` lists the third-party work with its own licences:
   Stockfish 19 (GPL-3.0-or-later, an unmodified official binary checked by SHA-256 and run as a
   separate process), Maia-2 (MIT, opt-in), the Syzygy tables (computed data, fetched and
   verified, never committed), the Lichess opening names (CC0), the cburnett pieces (BSD-3), the
   OFL fonts, Phosphor, chess.js and react-chessboard. `/licenses` shows Stockfish's licence notice
   from its `sf_19` source with a link to that source. `cargo deny` keeps third-party crates to
   permissive licences; the workspace crates are private and are skipped.
2. **Community files:** CONTRIBUTING, the Contributor Covenant 2.1 (reports through GitHub private
   reporting, no published email), SECURITY (private vulnerability reporting), SUPPORT, GOVERNANCE
   (one maintainer who merges, and how trust is earned), issue forms (bug, feature, coaching idea;
   blank issues off), a PR template, `CODEOWNERS`, and Dependabot (weekly, grouped, with a 7-day
   cooldown to match `check-npm-age.mjs` and `check-crate-age.sh`).
3. **The lifecycle** is `docs/SDLC.md`: issue, triage labels, fork and branch, PR, CI green,
   maintainer review, squash merge with a Conventional title, release.
4. **Protection as code** in `.github/settings/`:
   - `rulesets/main.json` (GitHub's REST ruleset schema) targets `~DEFAULT_BRANCH`: no deletion
     or force push, linear history, a pull request (0 approvals, conversations resolved, squash
     only), and strict required checks `ts`, `e2e`, `e2e-fakes`, `rust`, `baseline-schema`,
     `schema-drift` and `pr-title`, each pinned to the GitHub Actions app (id 15368). The only
     bypass actor is the repository admin role (id 5, always), so the maintainer alone can merge
     or push; contributors work from forks.
   - `repo.json`: squash merges only, titled by the PR, branches deleted on merge, Discussions on,
     wiki and projects off, the description, homepage and topics, private vulnerability reporting,
     and Pages built by Actions. `labels.json`: the triage labels.
   - `scripts/github/apply-settings.sh [--dry-run] [--prune]` reads the live state with `gh`,
     prints a diff, and writes only what differs (the ruleset found by name and updated in place,
     else created), so it is idempotent. It runs under Git Bash on Windows and on Linux. The
     maintainer runs it from their own login right after the repository is made public.
   - `scripts/github/test-settings.mjs` (in the `ts` job) validates the files and fails when the
     required checks and the job names in `ci.yml` and `pr-title.yml` part ways, so renaming a job
     cannot silently drop protection.
   - `Settings drift` (weekly, and on changes) compares what a read-only `GITHUB_TOKEN` can see:
     the ruleset's rules, its bypass list when visible, the labels and the public repo fields. It
     only notices while the repository is private.
5. **CI for a public repository:** `push` runs only on `main`, `pull_request` runs for every
   branch and fork, with concurrency that cancels superseded runs and read-only permissions. No
   workflow uses a secret.
6. **One product version and releases:**
   - The root `package.json` is the version; release-please keeps `apps/web/package.json`, the
     Cargo workspace version (a `x-release-please-version` marker) and the workspace crates in
     `Cargo.lock` (the `toml` updater on `$.package[?(!@.source)].version`, checked against the
     lock file with release-please 17.11.2) in step. `scripts/test-versions.mjs` fails CI if they
     differ.
   - SemVer with **every 0.y.z a beta**: `bump-minor-pre-major` and
     `bump-patch-for-minor-pre-major` are both on, so while the major is 0 a breaking change bumps
     the minor and `feat` or `fix` bump the patch. This keeps the minor for "you may need to change
     something" and lets features ship often without racing towards 1.0.
   - **1.0.0 is reserved for the MVP**, a deliberate maintainer decision in its own ADR, set with
     `release-as`, never reached by a commit message. GitHub Releases are prereleases while below
     1.0 (`prerelease: true`, which the version test ties to the major), tagged `v0.y.z`.
   - The first release is 0.1.0: the manifest starts at 0.0.0 with `release-as: 0.1.0`, and the
     changelog starts after `9717616` (`bootstrap-sha`), so the ~980 private commits are not
     replayed. `CHANGELOG.md` opens with a pre-history note pointing to this decision log.
   - The changelog sections read for players: Features, Fixes, Performance, Reverts and Docs;
     chore, test, ci, refactor, build and style are hidden.
   - The `Release` workflow (release-please-action v5.0.0, pinned by commit) is the only workflow
     with write permissions, `contents` and `pull-requests`, on its own `GITHUB_TOKEN`.
   - Squash merges use the PR title, so the `PR title` workflow
     (amannn/action-semantic-pull-request v6.1.2, pinned by commit) checks it on plain
     `pull_request` with a read token, never `pull_request_target`, and is a required check.
   - The app shows "Rematch v0.1.0 · beta" in the account menu, from the build's package version.

## Alternatives

- **MIT or Apache-2.0.** More permissive, but a hosted fork could close the source. AGPL keeps
  network deployments open, which suits a community-built coaching service.
- **Branch protection through the UI.** Not reviewable, not testable, and drifts silently.
- **An admin token in CI to apply settings.** A stored admin secret in a public repository is the
  largest risk the project could take; applying from the maintainer's login avoids it.
- **Starting at 1.0.0, or letting `feat` bump the minor.** Either would promise stability the beta
  does not have, or burn through minors with every feature.
- **Per-package versions.** The web app, worker and backend ship together as one product; one
  version is what players and bug reports need.

## Consequences

- Rulesets are only enforced once the repository is public: the maintainer runs
  `apply-settings.sh` right after the flip, then checks the drift workflow is green.
- Every new CI job must be added to `rulesets/main.json` in the same PR, or `ts` fails.
- The release PR is opened by `GITHUB_TOKEN`, which does not trigger other workflows, so its
  required checks don't report; the maintainer merges it with the admin bypass.
- After 0.1.0 ships, `release-as` must be removed from `release-please-config.json`
  (`test-versions.mjs` warns).
- The visual baselines and fixtures use the synthetic handle `sample_player` instead of a real
  third-party Lichess player; the games themselves are unchanged.
