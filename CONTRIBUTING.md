# Contributing to Rematch

Thank you for wanting to help. Rematch, chess improver, analyses your chess.com games with
Stockfish, finds the mistakes you keep repeating, explains them, and trains you on them. It is
built in the open, and contributions of every size are welcome: code, chess knowledge, design,
docs, testing, and bug reports.

This guide is honest about one thing up front: **the full stack is heavy.** It runs Postgres,
Redis, a Rust worker with Stockfish, the Next.js app and an AI sidecar in Docker. Plan on a
machine with **at least 16 GB of RAM** (give Docker around 12 GB of it) and 20 GB of free disk.
Many contributions need far less: docs, the UI with its fixtures, and most TypeScript tests run
without the full stack.

- [Ways to help](#ways-to-help)
- [Set up](#set-up)
- [Run the tests](#run-the-tests)
- [How we work](#how-we-work)
- [Versioning and releases](#versioning-and-releases)
- [Scope](#scope)
- [AI-assisted contributions](#ai-assisted-contributions)

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). How decisions are made is
in [GOVERNANCE.md](GOVERNANCE.md), and the whole lifecycle of a change is in
[docs/SDLC.md](docs/SDLC.md).

## Ways to help

- **Pick a [good first issue](https://github.com/Terkea/rematch/labels/good%20first%20issue)**
  or one marked [help wanted](https://github.com/Terkea/rematch/labels/help%20wanted). Comment
  on it first so two people don't build the same thing.
- **Share chess knowledge.** Strong players and coaches can open a
  [coaching idea](https://github.com/Terkea/rematch/issues/new?template=coaching_idea.yml). No
  code needed.
- **Report a bug** with the bug form. Please keep reports free of personal data.
- **Ask or propose** in [Discussions](https://github.com/Terkea/rematch/discussions) before a large
  change, so we agree on the shape before you spend a weekend on it.

## Set up

You need Git, Docker (Docker Desktop on Windows or macOS, with about 12 GB of memory assigned),
Node 22 (the exact version is in `.nvmrc`) and pnpm 12.

```sh
git clone https://github.com/<you>/rematch.git   # your fork
cd rematch
nvm use                                          # Node 22
corepack enable                                  # or: npm i -g pnpm@12
pnpm install
cp .env.example .env                             # host ports for Postgres and Redis
docker compose up -d --build                     # Postgres, Redis, worker with Stockfish, web, Mailpit
```

The app is then at <http://localhost:3100>. Sign-in emails go to Mailpit at
<http://localhost:48025>. The first build takes a while; later ones are cached.

Two optional pieces:

- **The AI coach** needs the `ai-sidecar` service and an `ANTHROPIC_API_KEY` in your `.env`.
  Without it, reports are computed only, which is fine for most work.
- **Maia** (move difficulty by rating) is opt-in: add `docker-compose.maia.yml` to `COMPOSE_FILE`.

The Rust backend is built and tested **only in Docker**, never on the host. Run this once per
Docker host to create the shared build caches:

```sh
sh scripts/rust-cache-init.sh
```

## Run the tests

We write the failing test first, then the code that makes it pass. Tests use Vitest (TypeScript)
and `cargo test` (Rust).

**TypeScript, on the host.** Start just Postgres and Redis (`docker compose up -d postgres redis`)
and point the tests at them. The web tests need a 32+ character `BETTER_AUTH_SECRET`; any random
value works.

```sh
pnpm --filter @ci/ui test
BETTER_AUTH_SECRET=$(openssl rand -hex 32) \
WEB_DATABASE_URL_TEST=postgres://postgres:postgres@127.0.0.1:55432/chess_web_test \
DATABASE_URL_TEST=postgres://postgres:postgres@127.0.0.1:55432/chess_test \
REDIS_URL=redis://127.0.0.1:56379 \
pnpm --filter @ci/web test -- --maxWorkers=2
```

On a machine with little free memory, `--maxWorkers=2` keeps Vitest from swapping. A single file
runs with `pnpm --filter @ci/web exec vitest run test/<name>.test.tsx`.

**The full TypeScript gate, as CI runs it**, inside the containers:

```sh
docker compose run --rm worker pnpm turbo run build lint typecheck test --concurrency=1 --filter=!@ci/web
docker compose run --rm web pnpm turbo run build lint typecheck test --concurrency=1 --filter=@ci/web
```

**Rust, in Docker:**

```sh
RUST="env COMPOSE_FILE=docker-compose.yml docker compose -p ci-dev run --rm rust-dev"   # no host ports
$RUST cargo test --locked -p chess-core        # one crate
$RUST sh -c "cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo deny --locked check && cargo test --locked --workspace"
COMPOSE_FILE=docker-compose.yml docker compose -p ci-dev down -v   # clean up when done
```

**Before you open a PR**, for the parts you touched:

```sh
pnpm lint && pnpm typecheck && pnpm format:check && pnpm design:check
node scripts/github/test-settings.mjs && node scripts/test-versions.mjs
```

`pnpm format` fixes formatting (Biome). End-to-end and visual tests run in a Linux `qa`
container; `CLAUDE.md` has the exact commands. CI runs all of it on your PR, so you don't need to
run the slow suites locally for a small change.

## How we work

- **Branch from `main` in your fork** and open a pull request. Nobody pushes to `main` directly.
- **Conventional Commits for PR titles.** The PR is squash-merged and its title becomes the commit
  on `main` and the changelog line: `feat(coach): cite the move number`,
  `fix(web): keep the filter on back`, `docs: explain the trainer`. Types: `feat`, `fix`, `perf`,
  `revert`, `docs`, `refactor`, `chore`, `test`, `ci`, `build`, `style`. A breaking change adds
  `!` (`feat!: ...`). The PR-title check enforces it.
- **Decisions get an ADR.** A choice that someone will later ask "why?" about goes in
  `docs/decisions/` as the next number, listed in that folder's README. Small is fine.
- **UI follows the design system.** `packages/ui/DESIGN.md` is the law: design tokens only, no
  raw colours, sizes or fonts, and `pnpm design:check` must pass. Attach screenshots at 390 px and
  1440 px, in day and night themes. Feature screenshots come from fixture data, never a real
  account.
- **Honest numbers.** Rematch states the sample size with every rate, shows a dash rather than a
  fake zero, and never claims "improved" unless the data shows it. Keep that true in what you build.
- **Dependencies** are pinned exactly and must be at least 7 days old (`scripts/check-npm-age.mjs`,
  `scripts/check-crate-age.sh`). Rust dependency changes are made by the maintainer.
- **No secrets or personal data** in code, fixtures, screenshots or issues.

The definition of done is in [docs/SDLC.md](docs/SDLC.md#definition-of-done).

## Versioning and releases

Rematch is one product with one version, kept in the root `package.json` (the web app and the
Rust workspace follow it). We use [Semantic Versioning](https://semver.org/) and every `0.y.z`
release is a **beta**:

- `feat` and `fix` bump the patch (`0.1.0` to `0.1.1`);
- a breaking change (`feat!:` or a `BREAKING CHANGE:` footer) bumps the minor (`0.1.1` to `0.2.0`);
- `1.0.0` is the MVP. It is a deliberate decision by the maintainer, recorded in an ADR, and never
  happens automatically.

[release-please](https://github.com/googleapis/release-please) reads the commit titles on `main`
(your PR titles) and keeps a release PR open with the next version and its `CHANGELOG.md` entry.
`feat` lands under Features, `fix` under Fixes, `perf` under Performance and `docs` under Docs;
`chore`, `test`, `ci`, `refactor`, `build` and `style` stay out of the changelog. When the
maintainer merges the release PR, it tags `v0.y.z` and publishes a GitHub Release marked as a
prerelease. So a clear PR title is a clear changelog.

## Scope

- **chess.com only, for now.** Rematch imports chess.com games. Lichess support exists in the code
  behind a flag and is a welcome future project, but please don't link real Lichess accounts in
  tests.
- **Self-hosted first.** There is no public hosted instance yet.

## AI-assisted contributions

AI-assisted PRs are welcome. The house rules for humans and agents are the same, and they are
written down: [`CLAUDE.md`](CLAUDE.md) and the skills in [`.claude/skills`](.claude/skills)
(the design system, chess UI conventions) describe how this repository is built and tested. Point
your assistant at them. You are responsible for every line you submit: read it, run the tests,
and make sure it does what the PR says.

## License

Rematch is licensed under the [GNU AGPL-3.0](LICENSE) (`AGPL-3.0-only`). By contributing, you
agree that your contribution is licensed under the same terms. Third-party components keep their
own licences; see [NOTICE](NOTICE).
