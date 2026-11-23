# How a change reaches Rematch

This is the life of a change, from an idea to a release. It is short on purpose. The why behind
it is [ADR 0095](decisions/0095-open-source-sdlc-and-releases.md).

```
issue -> triage -> fork and branch -> pull request -> CI green -> review -> squash merge -> release PR -> release
```

## 1. Issue

Everything starts as an issue or a Discussion, made from a form: a bug, a feature request (the
problem a player has, and the evidence the fix helps), or a coaching idea. Blank issues are off.
Questions go to Discussions Q&A; security problems go through private reporting
([SECURITY.md](../SECURITY.md)).

## 2. Triage

New issues get `needs-triage`. The maintainer then:

- adds a `type/*` label (`bug`, `feature`, `coaching-idea`, `docs`, `chore`, `security`) and one or
  more `area/*` labels (`analysis`, `coach`, `trainer`, `web-ui`, `rust-backend`, `docs`);
- marks it `good first issue` (small, well-described, with pointers) or `help wanted` when anyone
  can take it;
- removes `needs-triage`, or closes it with a reason (duplicate, out of scope, can't reproduce).

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
| `ts` | format drift, npm release age, the design ban lint, the settings and version tests, then build, lint, typecheck and test for every TypeScript package |
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
