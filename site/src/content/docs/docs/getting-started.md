---
title: Getting started
description: Run all of Rematch on your own machine with scripts/setup.sh, data and AI included, then sign in to a demo player or your own chess.com games.
sidebar:
  order: 1
---

`scripts/setup.sh` sets up the whole project in one command: the `.env`, the Docker stack, the data and the AI choice (ADR 0097). It is POSIX `sh`, so it runs in Git Bash on Windows and in a macOS or Linux terminal. It never installs anything with `sudo`; when something is missing it says what to install. Running it again repairs what is missing and skips what is done. This page and the set-up section of CONTRIBUTING.md share their text, so an edit goes in both.

Rematch is a beta (version 0.x) that you run yourself. There is no public sign-up yet.

## What you need

- **Docker** 24 or newer with **Compose** 2.24 or newer. On Windows and macOS that is Docker Desktop with about 12 GB of memory assigned; on Linux, Docker Engine and the Compose plugin.
- About 10 GB of free disk for the minimal path. On the dry run the images measured 3.3 GB and the volumes 1.8 GB, and the first Rust compile adds its build cache on top.
- Git.
- **Node** 22 and **pnpm** 12 only to work on the code or to run the coach on your own Claude Code login. The app itself needs Docker alone.

## The minimal path

```sh
git clone https://github.com/Terkea/rematch.git && cd rematch
sh scripts/setup.sh minimal --email=you@example.com
sh scripts/seed-demo.sh        # the same as pnpm seed:demo
```

1. Open <http://localhost:3100> and ask for a sign-in link with the address you gave.
2. The link arrives in **Mailpit**, a local mailbox, at <http://localhost:48025>. No mail leaves the machine.
3. Answer the age question. The demo player then fills every page (33 analysed games, the weaknesses, an example report and 3 trainer positions), and the header says "Demo data". `sh scripts/seed-demo.sh --remove` takes it away again; your login stays.

The script asks how the coach should run ([below](#the-ai-coach)); `--yes --ai=none` answers every question with the default. On the dry run, with Docker's build cache already warm, `setup.sh minimal` took 165 s. A first build on a clean machine compiles the Rust worker and takes longer; I have not timed that cold yet, and the weekly self-host workflow will record it.

`pnpm seed:demo` does the same as `sh scripts/seed-demo.sh`, but in a clone without `node_modules` **pnpm** 12 first installs the whole workspace on the host (about 20 s on the dry run). The shell script runs straight in the web container.

## Your own chess.com games

Link your chess.com username on the Accounts page. The worker imports your games and analyses every move with **Stockfish 19**; the overview fills in as games finish. Each game is about 39 CPU-seconds of engine time at the 1 million nodes a position the analysis uses, so a 90-day import of 300 rapid games takes about 49 minutes on 4 engines and about 25 on 8 (`WORKER_POOL_SIZE`, one core each), with the enrichment on top ([ADR 0113](/rematch/docs/decisions/0113-analyse-at-1-million-nodes/)). The script sets `ALLOW_UNVERIFIED_LINKS=1`, which locally treats your link as verified, so the coach works without the location code in your chess.com profile. Production refuses that setting.

## The full path: data included

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

## The AI coach

The script asks, or takes `--ai=local|key|none`. `sh scripts/setup.sh ai` switches later.

1. **`local`**: your own signed-in **Claude Code**, no key. The script checks that `claude` is on your PATH and that `claude auth status` reports a login (if not: run `claude` and type `/login`). It starts the sidecar on your machine in the background (`setup.sh ai-start`, `ai-stop`, `ai-status`; the pidfile and the log are in `.cache/`) and checks that `http://127.0.0.1:7310/health` reports `auth: "local"`. This mode is for your own local use only: a personal login must never serve other people (ADR 0062 Amendment 2), so a server that friends use needs a key.
2. **`key`**: your own **Anthropic** API key. The script asks for it with hidden input (or reads `$ANTHROPIC_API_KEY`), checks it with a models-list call that spends no tokens, and writes it only to `.env`, which git ignores. It never prints the key. A rejected key writes nothing.
3. **`none`**: reports show the computed numbers without the written commentary. Everything else works.

The script ends by printing the worker's `ai: enabled (...)` or `ai: disabled (<reason>)` line.

## Flags

- `--yes` never asks. Without `--email` it invites `player@rematch.test`, a reserved address that only Mailpit sees.
- `--web-port=N`, `--mailpit-port=N`, `--postgres-port=N` and `--redis-port=N` move the host ports, and the sign-in links follow the web port. `--no-ports` publishes none.
- `--project=NAME` runs a second copy beside the first (`COMPOSE_PROJECT_NAME` in `.env`).
- `sh scripts/setup.sh doctor` re-checks the prerequisites, the configuration, each service, the worker's AI line and the data, and explains each failure.

## Windows, macOS and Linux

- Windows: run the script in Git Bash, which comes with Git for Windows, not in PowerShell or cmd. Docker Desktop runs on WSL 2, whose memory is set in `%UserProfile%\.wslconfig` (`[wsl2]`, then `memory=12GB`) and applied with `wsl --shutdown`. The dry runs were on Windows 11.
- macOS: Docker Desktop, Settings, Resources, Memory: 12 GB. On Apple Silicon the script sets `DOCKER_DEFAULT_PLATFORM=linux/amd64`, because the images pin an x86-64 build of **Stockfish**, so the stack runs under emulation. I have not run it on Apple hardware, so I don't know yet how slow that is.
- Linux: Docker Engine and the Compose plugin, with your user in the `docker` group so the script runs without `sudo`. The data service writes `data/` as your user. The weekly self-host workflow runs the minimal path on ubuntu-latest.

## Troubleshooting

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

## Stop or reset

```sh
docker compose down        # stop, and keep the data
docker compose down -v     # stop, and delete the database and the volumes
sh scripts/setup.sh ai-stop
```

## Next

- [What Rematch does](/rematch/docs/features/), feature by feature.
- [Architecture](/rematch/docs/architecture/), before you change it.
- [Contributing](/rematch/docs/contributing/): the tests, the gates and how a change gets merged.
