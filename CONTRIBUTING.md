# Contributing to Rematch

Rematch, chess improver, analyses your chess.com games with **Stockfish**, finds the mistakes you
keep repeating, explains them and trains you on them. Code, chess knowledge, design, docs and
bug reports all help, and small pull requests are reviewed fastest. Many thanks for considering it.

The full stack is heavy. It runs **Postgres**, **Redis**, a Rust worker with **Stockfish**, the
**Next.js** app and an AI sidecar in Docker, so plan on a machine with at least 16 GB of RAM, 12 GB
of it for Docker, and about 10 GB of free disk. Docs, the UI with its fixtures and most TypeScript
tests need far less.

- [Ways to help](#ways-to-help)
- [Set up](#set-up)
- [Run the tests](#run-the-tests)
- [How we work](#how-we-work)
- [Versioning and releases](#versioning-and-releases)
- [Scope](#scope)
- [AI-assisted contributions](#ai-assisted-contributions)
- [How we review outside code](#how-we-review-outside-code)

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). How decisions are made is
in [GOVERNANCE.md](GOVERNANCE.md), and the whole lifecycle of a change is in
[docs/SDLC.md](docs/SDLC.md).

## Ways to help

- Pick a [good first issue](https://github.com/Terkea/rematch/labels/good%20first%20issue)
  or one marked [help wanted](https://github.com/Terkea/rematch/labels/help%20wanted). Comment
  on it first so two people don't build the same thing.
- Share chess knowledge. Strong players and coaches can open a
  [coaching idea](https://github.com/Terkea/rematch/issues/new?template=coaching_idea.yml). No
  code needed. Rematch is for anyone who knows how the pieces move, up to about 2000. Above 2000
  it hasn't been tested with real players yet; if you're 2000+, tell us where it's wrong.
- Report a bug with the bug form. Please keep reports free of personal data.
- Ask or propose in [Discussions](https://github.com/Terkea/rematch/discussions) before a large
  change, so we agree on the shape before you spend a weekend on it.

## Set up

`scripts/setup.sh` sets up the whole project in one command: the `.env`, the Docker stack, the data and the AI choice (ADR 0097). It is POSIX `sh`, so it runs in Git Bash on Windows and in a macOS or Linux terminal. It never installs anything with `sudo`; when something is missing it says what to install. Running it again repairs what is missing and skips what is done. The same text is on the site as [Getting started](https://terkea.github.io/rematch/docs/getting-started/).

### What you need

- **Docker** 24 or newer with **Compose** 2.24 or newer. On Windows and macOS that is Docker Desktop with about 12 GB of memory assigned; on Linux, Docker Engine and the Compose plugin.
- About 10 GB of free disk for the minimal path. On the dry run the images measured 3.3 GB and the volumes 1.8 GB, and the first Rust compile adds its build cache on top.
- Git.
- **Node** 22 and **pnpm** 12 only to work on the code or to run the coach on your own Claude Code login. The app itself needs Docker alone.

### The minimal path

```sh
git clone https://github.com/<you>/rematch.git && cd rematch   # your fork
sh scripts/setup.sh minimal --email=you@example.com
sh scripts/seed-demo.sh        # the same as pnpm seed:demo
```

1. Open <http://localhost:3100> and ask for a sign-in link with the address you gave.
2. The link arrives in **Mailpit**, a local mailbox, at <http://localhost:48025>. No mail leaves the machine.
3. Answer the age question. The demo player then fills every page (33 analysed games, the weaknesses, an example report and 3 trainer positions), and the header says "Demo data". `sh scripts/seed-demo.sh --remove` takes it away again; your login stays.

The script asks how the coach should run (below); `--yes --ai=none` answers every question with the default. On the dry run, with Docker's build cache already warm, `setup.sh minimal` took 165 s. A first build on a clean machine compiles the Rust worker and takes longer; I have not timed that cold yet, and the weekly self-host workflow will record it.

`pnpm seed:demo` does the same as `sh scripts/seed-demo.sh`, but in a clone without `node_modules` **pnpm** 12 first installs the whole workspace on the host (about 20 s on the dry run). The shell script runs straight in the web container.

### Your own chess.com games

Link your chess.com username on the Accounts page. The worker imports your games and analyses every move with **Stockfish 19**; the overview fills in as games finish. The script sets `ALLOW_UNVERIFIED_LINKS=1`, which locally treats your link as verified, so the coach works without the location code in your chess.com profile. Production refuses that setting.

### The full path: data included

```sh
sh scripts/setup.sh full             # add --maia for move difficulty by rating
sh scripts/setup.sh data             # the same data, added to a stack that is already up
```

Every download shows its size before it starts and is checksummed.

| Data | Size | Time on the dry run | Without it |
|---|---|---|---|
| **Syzygy** 3-4-5 endgame tables (`data/syzygy`) | 290 files, 984 MB (940 MiB on disk) | 101 s; a re-run re-hashes them in 9 s | no tablebase probe; analysis goes on |
| A **Lichess** month for the opening explorer: 2013-01, the default | 18 MB streamed, 121,332 games | about 40 s, 59,585 explorer rows | the opening explorer is empty |
| A **Lichess** month for the band reference: 2017-04 or later | 2017-04: 3.5 GB streamed, 11.3M games. 2026-10: 31 GB, 95.4M games | not measured; afterwards the worker analyses the reference games in the background | weaknesses say "Not compared yet" |
| **Maia**-2 difficulty service (`--maia`) | its image holds **PyTorch** (CPU) and the weights | not measured on this run | difficulty stays "pending", never estimated |

- The Syzygy tables come from the **Lichess** tablebase mirror. Each file is checked against the pinned `backend/crates/lichess-db/data/syzygy-345.sha256` (ADR-R2-22). A stopped fetch keeps the verified files and fetches the rest.
- The Lichess month comes from the open **Lichess** database (CC0). It is streamed, never stored, and verified against Lichess's own `sha256sums.txt`. A stopped build starts that month again.
- The band reference needs `[%clk]` comments, and Lichess dumps carry them from 2017-04 on. The 2013-01 default therefore builds the explorer only, and the script says so. For peer comparisons, run `sh scripts/setup.sh data --month=2017-04`. `--no-lichess` skips the month entirely.
- The **Maia**-2 weights (ADR-R2-13) are downloaded and SHA-256-checked by `maia2` when the image is built.

### The AI coach

The script asks, or takes `--ai=local|key|none`. `sh scripts/setup.sh ai` switches later.

1. **`local`**: your own signed-in **Claude Code**, no key. The script checks that `claude` is on your PATH and that `claude auth status` reports a login (if not: run `claude` and type `/login`). It starts the sidecar on your machine in the background (`setup.sh ai-start`, `ai-stop`, `ai-status`; the pidfile and the log are in `.cache/`) and checks that `http://127.0.0.1:7310/health` reports `auth: "local"`. This mode is for your own local use only: a personal login must never serve other people (ADR 0062 Amendment 2), so a server that friends use needs a key.
2. **`key`**: your own **Anthropic** API key. The script asks for it with hidden input (or reads `$ANTHROPIC_API_KEY`), checks it with a models-list call that spends no tokens, and writes it only to `.env`, which git ignores. It never prints the key. A rejected key writes nothing.
3. **`none`**: reports show the computed numbers without the written commentary. Everything else works.

The script ends by printing the worker's `ai: enabled (...)` or `ai: disabled (<reason>)` line.

### Flags

- `--yes` never asks. Without `--email` it invites `player@rematch.test`, a reserved address that only Mailpit sees.
- `--web-port=N`, `--mailpit-port=N`, `--postgres-port=N` and `--redis-port=N` move the host ports, and the sign-in links follow the web port. `--no-ports` publishes none.
- `--project=NAME` runs a second copy beside the first (`COMPOSE_PROJECT_NAME` in `.env`).
- `sh scripts/setup.sh doctor` re-checks the prerequisites, the configuration, each service, the worker's AI line and the data, and explains each failure.

### Windows, macOS and Linux

- Windows: run the script in Git Bash, which comes with Git for Windows, not in PowerShell or cmd. Docker Desktop runs on WSL 2, whose memory is set in `%UserProfile%\.wslconfig` (`[wsl2]`, then `memory=12GB`) and applied with `wsl --shutdown`. The dry runs were on Windows 11.
- macOS: Docker Desktop, Settings, Resources, Memory: 12 GB. On Apple Silicon the script sets `DOCKER_DEFAULT_PLATFORM=linux/amd64`, because the images pin an x86-64 build of **Stockfish**, so the stack runs under emulation. I have not run it on Apple hardware, so I don't know yet how slow that is.
- Linux: Docker Engine and the Compose plugin, with your user in the `docker` group so the script runs without `sudo`. The data service writes `data/` as your user. The weekly self-host workflow runs the minimal path on ubuntu-latest.

### Troubleshooting

These are the snags from my dry runs on Windows 11 (Git Bash, Docker Desktop 29.1, Compose 5.0). Most come from running `docker compose` by hand instead of `setup.sh`; the last one is from the first full-mode run.

| What you see | Why, and the fix |
|---|---|
| `external volume "ci-cargo-registry" not found`, and nothing starts | `docker compose up` ran before the shared Rust caches existed. `sh scripts/rust-cache-init.sh` creates them once per Docker host; `setup.sh` runs it first. |
| `Bind for 127.0.0.1:48025 failed: port is already allocated` | Another stack, often a second copy of Rematch, holds the port. `sh scripts/setup.sh configure --web-port=3101 --mailpit-port=48026 --postgres-port=55440 --redis-port=56380`, or `--no-ports`. |
| The sign-in page says "Check your email", but Mailpit stays empty | Sign-up is invite-only, and `INVITE_ALLOWLIST` was empty. The app does not tell an uninvited address apart, by design. `sh scripts/setup.sh configure --email=you@example.com`, then `docker compose up -d web`. |
| `docker compose up -d` also starts `rust-dev`, which exits at once, and `worker`, the old TypeScript worker | A plain `up` starts every service in the file. `worker` stays in the file because CI runs the TypeScript gate in its image (ADR 0067). `setup.sh` starts only what the app uses: `docker compose up -d --wait postgres redis mailpit web ai-sidecar rust-worker`. |
| `nvm use` fails with "A version argument is required but missing" | nvm-windows does not read `.nvmrc`. Run `nvm use 22`. |
| The worker logs `no Syzygy .rtbw files found; tablebase probes disabled` | Expected in minimal mode. `sh scripts/setup.sh data` fetches the tables, and the worker logs `"tablebase probe" ... "status":"available","tables":145` after its restart. |
| The worker logs `ai: disabled (sidecar has no credential)` | Expected with `--ai=none`. `sh scripts/setup.sh ai` switches to `local` or `key`. |
| `lichess-db` fails with `config: no reference player qualified in any cell` | The month predates `[%clk]` comments (before 2017-04), so no player qualifies for the band reference. The explorer from that month is still built. `setup.sh` builds the band reference only from 2017-04 on. |

### Working on the code

`setup.sh` leaves the host untouched. For your editor and the host-side tests, install the workspace:

```sh
nvm use 22         # nvm-windows needs the version; elsewhere `nvm use` reads .nvmrc
corepack enable    # or: npm i -g pnpm@12
pnpm install
```

The Rust backend is built and tested only in Docker, never on the host (see below).

## Run the tests

We write the failing test first, then the code that makes it pass. Tests use Vitest (TypeScript)
and `cargo test` (Rust).

TypeScript, on the host. Start only Postgres and Redis (`docker compose up -d postgres redis`)
and point the tests at them. Ports 55432 and 56379 are published when `.env` has the ports
overlay in `COMPOSE_FILE`, as `setup.sh` writes it; use your own if you moved them with
`--postgres-port` or `--redis-port`. The web tests need a 32+ character `BETTER_AUTH_SECRET`; any random
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

The full TypeScript gate, as CI runs it, inside the containers:

```sh
docker compose run --rm worker pnpm turbo run build lint typecheck test --concurrency=1 --filter=!@ci/web
docker compose run --rm web pnpm turbo run build lint typecheck test --concurrency=1 --filter=@ci/web
```

Rust, in Docker:

```sh
RUST="env COMPOSE_FILE=docker-compose.yml docker compose -p ci-dev run --rm rust-dev"   # no host ports
$RUST cargo test --locked -p chess-core        # one crate
$RUST sh -c "cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo deny --locked check && cargo test --locked --workspace && sh scripts/check-crate-age.sh"
COMPOSE_FILE=docker-compose.yml docker compose -p ci-dev down -v   # clean up when done
```

Before you open a PR, for the parts you touched:

```sh
pnpm lint && pnpm typecheck && pnpm format:check && pnpm design:check
node scripts/github/test-settings.mjs && node scripts/test-versions.mjs
```

`pnpm format` fixes formatting (Biome). End-to-end and visual tests run in a Linux `qa`
container; `CLAUDE.md` has the exact commands. CI runs all of it on your PR, so you don't need to
run the slow suites locally for a small change.

## How we work

- Branch from `main` in your fork and open a pull request. Nobody pushes to `main` directly.
- Conventional Commits for PR titles. The PR is squash-merged and its title becomes the commit
  on `main` and the changelog line: `feat(coach): cite the move number`,
  `fix(web): keep the filter on back`, `docs: explain the trainer`. Types: `feat`, `fix`, `perf`,
  `revert`, `docs`, `refactor`, `chore`, `test`, `ci`, `build`, `style`. A breaking change adds
  `!` (`feat!: ...`). The PR-title check enforces it.
- Decisions get an ADR. A choice that someone will later ask "why?" about goes in
  `docs/decisions/` as the next number, listed in that folder's README. Small is fine.
- UI follows the design system. `packages/ui/DESIGN.md` is the law: design tokens only, no
  raw colours, sizes or fonts, and `pnpm design:check` must pass. Attach screenshots at 390 px and
  1440 px, in day and night themes. Feature screenshots come from fixture data, never a real
  account.
- Honest numbers. Rematch states the sample size with every rate, shows a dash rather than a
  fake zero, and never claims "improved" unless the data shows it. Keep that true in what you build.
- Dependencies are pinned exactly and must be at least 7 days old (`scripts/check-npm-age.mjs`,
  `backend/scripts/check-crate-age.sh`). For now the maintainer makes Rust dependency changes.
- No secrets or personal data in code, fixtures, screenshots or issues.

The definition of done is in [docs/SDLC.md](docs/SDLC.md#definition-of-done).

## Versioning and releases

Rematch is one product with one version, kept in the root `package.json` (the web app and the
Rust workspace follow it). We use [Semantic Versioning](https://semver.org/) and every `0.y.z`
release is a beta:

- `feat` and `fix` bump the patch (`0.1.0` to `0.1.1`);
- a breaking change (`feat!:` or a `BREAKING CHANGE:` footer) bumps the minor (`0.1.1` to `0.2.0`);
- `1.0.0` is the MVP. It is a deliberate decision by the maintainer, recorded in an ADR, and never
  happens automatically.

[release-please](https://github.com/googleapis/release-please) reads the commit titles on `main`
(your PR titles) and keeps a release PR open with the next version and its `CHANGELOG.md` entry.
`feat` lands under Features, `fix` under Fixes, `perf` under Performance, `revert` under Reverts and
`docs` under Docs;
`chore`, `test`, `ci`, `refactor`, `build` and `style` stay out of the changelog. When the
maintainer merges the release PR, it tags `v0.y.z` and publishes a GitHub Release marked as a
prerelease. So a clear PR title is a clear changelog.

## Scope

- chess.com only, for now. Rematch imports chess.com games. Lichess support exists in the code
  behind a flag and is a welcome future project, but please don't link real Lichess accounts in
  tests.
- Self-hosted first. There is no public hosted instance yet.

## AI-assisted contributions

AI-assisted PRs are welcome. The house rules for humans and agents are the same, and they are
written down: [`CLAUDE.md`](CLAUDE.md) and the skills in [`.claude/skills`](.claude/skills)
(the design system, chess UI conventions) describe how this repository is built and tested. Point
your assistant at them. You are responsible for every line you submit: read it, run the tests,
and make sure it does what the PR says.

## How we review outside code

Every pull request from outside the project is reviewed as untrusted code, yours included; it
is nothing personal, and it is the same for everyone (ADR 0108, [SECURITY.md](SECURITY.md#how-we-review-outside-code)).

- Your pull request's workflows run once the maintainer approves them. They run on
  `pull_request`, never `pull_request_target`, with a read-only token and no secrets.
- The `Sensitive paths` check fails when you change `.github/`, a Dockerfile or compose file,
  `deploy/`, a lockfile, `package.json` scripts or dependencies, package-manager or toolchain
  configuration, a `build.rs`, a `scripts/` directory, `CLAUDE.md`, `AGENTS.md`, `.claude/`,
  `.mcp.json`, the coach's prompts (`backend/crates/ai/prompts/`), `algorithm/` or
  `SECURITY.md`. That is expected: the maintainer reads those files line by line and then adds
  the `sensitive-reviewed` label. Tick the template's "touches sensitive paths" box and say why
  the change needs them; keeping them in a pull request of their own makes the review faster.
- `CLAUDE.md`, `.claude/` and the prompts are on the list because AI agents read them as
  instructions. A sentence there is prompt injection into every agent that works on the
  repository, or into the coach.
- The maintainer never runs your branch on the host, only inside the project's containers.

## Licence

Rematch is licensed under the [GNU AGPL-3.0](LICENSE) (`AGPL-3.0-only`). By contributing, you
agree that your contribution is licensed under the same terms. Third-party components keep their
own licences; see [NOTICE](NOTICE).
