# How a change reaches Rematch

This is the life of a change, from an idea to a release. It is short on purpose. The why behind
it is [ADR 0095](decisions/0095-open-source-sdlc-and-releases.md); the roadmap and the
maintenance sweeps are [ADR 0099](decisions/0099-roadmap-and-maintenance-sweeps.md).

```
idea -> triage -> needs-spec -> spec -> accepted -> milestone -> fork and branch -> pull request
  -> CI green -> review -> squash merge -> release PR -> release
```

## 1. Issue

Everything starts as an issue or a Discussion, made from a form: a bug, a feature request (the
problem a player has, and the evidence the fix helps), a coaching idea, or a feature spec. Blank
issues are off. Questions go to Discussions Q&A; security problems go through private reporting
([SECURITY.md](../SECURITY.md)).

## 2. Triage

New issues get `needs-triage`. The maintainer then:

- adds a `type/*` label (`bug`, `feature`, `coaching-idea`, `docs`, `chore`, `security`) and one or
  more `area/*` labels (`analysis`, `coach`, `trainer`, `web-ui`, `rust-backend`, `infra`, `docs`);
- marks it `good first issue` (small, well-described, with pointers) or `help wanted` when anyone
  can take it;
- removes `needs-triage`, or closes it with a reason (duplicate, out of scope, can't reproduce).

A bug with a clear fix goes straight to a pull request. A feature goes through a spec first:

1. `needs-spec`: the problem is accepted, the design is not. The idea stays open with this label.
2. A spec issue, from the "Feature spec (RFC)" form (`spec` label), opened by the maintainer or
   whoever wants to build it: the problem with a real example, who it is for, goals and non-goals,
   the evidence, the design (UX, data, algorithm), checkable acceptance criteria, the test plan,
   whether it needs an ADR, the risks, and the files to start from. Where a spec or plan already
   exists in `docs/superpowers/`, the issue links it instead of copying it.
3. `accepted`: after review the maintainer adds the label and a milestone. A large spec becomes an
   epic (`epic` label), and its parts become sub-issues, each closed by its own pull requests.

The labels are kept as code in `.github/settings/labels.json`.

## 3. Fork and branch

Contributors work in a fork; branches are named for what they do (`fix/filter-on-back`,
`feat/streak-copy`). Comment on the issue first, so two people don't build the same thing.

## 4. Pull request

- The title is a Conventional Commit (`feat(coach): ...`, `fix(web): ...`). It becomes the commit
  on `main` and the changelog line, and the `pr-title` check enforces it.
- The template's checklist applies: test first, the gates, an ADR when a decision was made,
  `design:check` and screenshots for UI, no secrets or personal data.
- Small pull requests are reviewed faster. One idea per PR.

## 5. CI

Every pull request runs the same checks, and all of them are required by the `main` ruleset:

| Check | What it proves |
| --- | --- |
| `ts` | format drift, npm release age, the design ban lint, the settings, version and sweep tests, the README roadmap check, then build, lint, typecheck and test for every TypeScript package |
| `e2e` | the Playwright suite and the visual baselines on a production build |
| `e2e-fakes` | the specs that need the chess.com and Lichess fakes, on the dev server |
| `rust` | crate age, `cargo fmt`, `clippy -D warnings`, `cargo deny`, and `cargo test` |
| `site` | the public site's tests, its production build and a check of every internal link |
| `baseline-schema` | the Rust (sqlx) and Drizzle migrations produce the same schema |
| `schema-drift` | the generated Drizzle schema is up to date with `db/migrations` |
| `pr-title` | the title is a Conventional Commit |

CI uses no secrets, and fork pull requests run with a read-only token. A pull request must be up
to date with `main` before it merges (strict checks).

## 6. Review

The maintainer reviews every pull request (`CODEOWNERS`). No approval count is required, because
there is one maintainer, but every review conversation must be resolved before merging.

## 7. Merge

Squash merge only, with the PR title as the commit title, and linear history. The branch is
deleted on merge. Only a repository admin (the maintainer) can merge or push to `main`; nobody
can force-push it or delete it. These rules live in `.github/settings/rulesets/main.json` and are
applied by `scripts/github/apply-settings.sh`; the `Settings drift` workflow checks them weekly.

## 8. Versioning and releases

Rematch is one product with **one version**: the root `package.json`. release-please keeps
`apps/web/package.json`, the Cargo workspace version and its crates in `Cargo.lock` in step with
it, and `scripts/test-versions.mjs` fails CI when they differ. The app shows the version in the
account menu ("Rematch v0.1.0 · beta").

Versions follow [Semantic Versioning](https://semver.org/):

- **every `0.y.z` is a beta.** While the major is 0, `feat` and `fix` bump the patch, and a breaking
  change (`feat!:`, or a `BREAKING CHANGE:` footer) bumps the minor
  (`bump-patch-for-minor-pre-major` and `bump-minor-pre-major` in `release-please-config.json`);
- **`1.0.0` is the MVP.** It is a deliberate maintainer decision, made in its own ADR, and is set by
  hand with `release-as`. It never happens because of a commit message.

How a release happens:

1. Each push to `main` runs the `Release` workflow. release-please reads the Conventional Commit
   titles since the last release and opens, or updates, one release PR: the next version in every
   versioned file and the new `CHANGELOG.md` entry.
2. The changelog groups `feat` under Features, `fix` under Fixes, `perf` under Performance and
   `docs` under Docs. `chore`, `test`, `ci`, `refactor`, `build` and `style` are left out, so the
   changelog reads for players.
3. When the maintainer merges the release PR, release-please tags `v0.y.z` and publishes a GitHub
   Release, marked **prerelease** while the version is below 1.0.

Notes for the maintainer:

- The release workflow is the only one that writes, with its own `GITHUB_TOKEN`
  (`contents: write`, `pull-requests: write`). GitHub does not run other workflows on a PR that
  token opens, so the required checks don't report on the release PR: merge it with the admin
  bypass, after the commits it lists passed CI on `main`.
- The first release is 0.1.0 (`release-as` in the config, with the changelog starting after
  commit `9717616`). Once it is out, remove `release-as`; `test-versions.mjs` warns until you do.
- At 1.0.0, set `"prerelease": false` in the config in the same change (the version test requires
  it).

## Roadmap

The roadmap is GitHub's milestones and issues; nothing else holds it.

- Milestones are versions with a theme: `v0.2 Practice habit`, `v0.3 Trainer depth`,
  `v0.4 Self-host and benchmark`, `v0.5 More players` and `v1.0 MVP`. They are kept as code in
  `.github/settings/milestones.json`, each with a description and a horizon (`now`, `next` or
  `later`), and `scripts/github/apply-settings.sh` creates or updates them by title. It deletes one
  only with `--prune`.
- Epics are one issue per theme with the full spec, and their work is linked to them as
  [sub-issues](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues),
  so each epic shows its progress, done of total.
- The seed is `.github/settings/roadmap-issues.json`: the first epics and sub-issues, with their
  labels, size, milestone and parent. `apply-settings.sh --seed-issues` creates the ones whose title
  does not exist yet (open or closed), epics first, then links every sub-issue under its epic; a
  second run does nothing. Once an issue exists, GitHub is its source of truth and the file is not
  synced back. `test-settings.mjs` checks that every milestone and label exists, the titles are
  unique, every parent is an epic in the same milestone, every epic has the spec's sections in
  order, and every good first issue is size S.
- The site builds its roadmap page and the landing page's Now, Next and Later from the public API
  (the milestones, and each epic's `sub_issues_summary`) at build time, daily and on every push to
  `main`. When the API cannot be read (the repository is private, the build is offline, or the rate
  limit is spent), it falls back to the seed files and shows counts only, never invented progress.
- The README carries a short summary between the `roadmap:start` and `roadmap:end` markers, written
  by `node scripts/roadmap/render-readme.mjs` from the seed files. The `ts` job runs it with
  `--check` and fails when the summary is stale.
- The board is a public GitHub Project, "Rematch roadmap", with Status, Milestone and Area.
  `scripts/github/create-project.sh` creates it or brings it up to date (idempotent, with
  `--dry-run`). It needs the `project` scope once: `gh auth refresh -s project`.

To re-rank, move epics between milestones on GitHub and milestones between horizons in
`milestones.json`, then run `apply-settings.sh` and the README renderer. The monthly sweep asks for
this.

## Maintenance sweeps

Scheduled workflows keep the repository honest between changes. None uses a secret: each has only
the run's own `GITHUB_TOKEN`, with `issues: write` only in the job that files the issue, and every
action is pinned by commit SHA.

| Workflow | When | What it does |
| --- | --- | --- |
| `Weekly sweep` | Mondays | Checks every link in the README, docs and site (lychee; localhost and GitHub's rate-limited pages are skipped), runs `cargo deny check advisories` against the day's RustSec database, fails on an advisory ignore past its `# expires:` date, runs the crate-age rule, the README roadmap check and the decision log's index check, and reports `pnpm audit --prod` without failing on it. |
| `Stale` | Tuesdays | Labels issues with no activity for 60 days, and pull requests for 30, as `stale`. Issues close 30 days later without a reply; pull requests are never closed by it. Exempt: `good first issue`, `help wanted`, `roadmap`, `epic`, `spec`, `accepted`, security, anything in a milestone and anything assigned. |
| `Monthly sweep` | the 1st | Opens "Maintenance sweep: YYYY-MM" from `.github/sweeps/monthly-maintenance.md`: stale ADRs, the Node, Rust, Stockfish, Maia and Syzygy pins, the visual baselines, docs accuracy, the benchmark trend, the roadmap ranking, triage and the security policy. |
| `Settings drift` | Mondays | Compares the live repository with `.github/settings` (ADR 0095). |
| `Pages` | daily | Rebuilds the site, so the roadmap follows the issues. |

When the weekly sweep fails, it opens one issue, "Weekly sweep: <date>", with the findings. If one
is already open, the sweep updates its title and body and adds a comment instead of filing a second
one. The first sweep that passes closes it. How to fix what it finds:

- A broken link: fix or remove it. For a site that refuses robots, add an `--exclude` to the
  workflow with a comment saying why.
- An advisory: bump the crate. That is a maintainer change, because `Cargo.toml` and `Cargo.lock`
  belong to the maintainer. When no fixed version is 7 days old yet, add an ignore to
  `backend/deny.toml` with a reason and `# expires: YYYY-MM-DD` on the same line.
- Crate age, a stale README roadmap or the decision log: the message names the file and the
  command that fixes it.

Dependabot (`.github/dependabot.yml`) opens grouped weekly updates for npm, Cargo, Docker and
GitHub Actions, with a 7-day cooldown that matches `check-npm-age.mjs` and `check-crate-age.sh`.
Majors come one per pull request. A Cargo update is reviewed and merged by the maintainer only.

## Definition of done

A change is done when:

- the behaviour has a test that was written first and failed for the right reason;
- the gates for what it touches pass (CI runs them all: lint, typecheck, format, the design ban
  lint, unit, e2e and visual tests, the Rust gate, the settings and version tests);
- UI follows `packages/ui/DESIGN.md`, with screenshots at 390 px and 1440 px in day and night,
  every state covered (loading, empty, error, long content), and reduced motion respected;
- numbers are honest: a sample size with every rate, a dash rather than a fake zero, no
  "improved" without evidence;
- a decision worth remembering has an ADR, and docs that describe the behaviour are updated;
- there are no secrets, tokens, personal data or other players' private games anywhere in it;
- the PR title is a Conventional Commit that describes the change for the changelog.
