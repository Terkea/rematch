---
title: Getting started
description: Run all of Rematch on your own machine with one command, data and AI included, then sign in and see a demo player or your own chess.com games.
sidebar:
  order: 1
---

Rematch is a beta (version 0.x) that you run yourself. There is no public sign-up yet. One script, `scripts/setup.sh`, sets up the whole project: the configuration, the Docker stack, the data it needs and how the AI coach runs. It is plain POSIX `sh`, so it runs in Git Bash on Windows and in any macOS or Linux terminal. It never installs anything with `sudo`; when something is missing, it tells you what to install. Running it again repairs what is missing and skips what is done.

## What you need

- **Docker** with Compose 2.24 or newer: Docker Desktop on Windows or macOS with about **12 GB of memory** assigned, or Docker Engine and the Compose plugin on Linux.
- About **20 GB of free disk** (24 GB for the full data, 30 GB with Maia).
- **Git.**
- Node 22 and pnpm 12 only if you want to work on the code, or to use your own Claude Code login for the coach. Running the app needs Docker alone.

## The minimal path: three commands

```sh
git clone https://github.com/Terkea/rematch.git && cd rematch
sh scripts/setup.sh minimal --email=you@example.com
pnpm seed:demo        # no pnpm? sh scripts/seed-demo.sh does the same
```

The first run builds the images (it compiles the Rust worker with Stockfish 19 and installs the web app): 10 to 30 minutes and about 12 GB. Later runs take seconds. The script asks how the coach should run (see [The AI coach](#the-ai-coach)); pass `--yes --ai=none` to skip every question.

Then:

1. Open <http://localhost:3100> and ask for a sign-in link with the address you gave.
2. The email goes to Mailpit, a local mailbox, at <http://localhost:48025>. Nothing leaves your machine.
3. The demo player fills every page: analysed games, weaknesses, an example report, the trainer. The header says **Demo data**. `pnpm seed:demo --remove` takes it away.

## Your own chess.com games

Link your chess.com username on the Accounts page. Your games import, and the worker analyses every move with Stockfish 19; the overview fills in as games finish. The script sets `ALLOW_UNVERIFIED_LINKS=1`, which locally treats your link as verified, so you see your coach without putting a code in your chess.com profile. Production refuses that setting.

## The full path: data included

```sh
sh scripts/setup.sh full            # add --maia for move difficulty by rating
```

Full mode is minimal plus the data below. `sh scripts/setup.sh data` adds it later to a stack that is already running. Every download shows its size before it starts and is checksummed.

| Data | Mode | Size | Time | Without it |
|---|---|---|---|---|
| Docker images | both | about 12 GB | 10 to 30 min once | nothing runs |
| The demo player | optional | under 1 MB | seconds | pages stay empty until your games are analysed |
| Syzygy 3-4-5 endgame tables | full | 984 MB, 290 files | about 5 min at 50 Mbit/s | no tablebase probe; analysis goes on |
| A Lichess month (the opening explorer and the rating-band reference) | full | 2013-01: 18 MB streamed, 121k games. A recent month: about 31 GB, 95M games | 2013-01: about a minute, then its reference games are analysed in the background. A recent month: hours | the explorer is empty and weaknesses say "Not compared yet" |
| Maia-2 difficulty | full, `--maia` | about 3 GB of images | 5 to 10 min to build | difficulty stays "pending", never estimated |

- **Syzygy** files are checked one by one against the pinned `syzygy-345.sha256` manifest. A stopped fetch keeps the verified files and resumes with the rest.
- **The Lichess month** is streamed, never stored, and verified against Lichess's own `sha256sums.txt`. A stopped build starts that month again. The default month, 2013-01, is small so the first run stays quick; it has fewer players per rating band, so peer comparisons are coarser. Pick another with `--month=YYYY-MM`, or skip it with `--no-lichess`.
- **Maia's** weights are downloaded and SHA-256-checked by `maia2` when its image is built.

## The AI coach

The script asks, or takes `--ai=local|key|none`. Switch any time with `sh scripts/setup.sh ai`.

- **`local`**: your own signed-in Claude Code, no key. The script checks that `claude` is installed and signed in (if not: run `claude`, then type `/login`), starts a small sidecar on your machine (`setup.sh ai-start`, `ai-stop`, `ai-status`) and checks that it reports `auth: "local"`. This is for your own local use only: a personal login must never serve other people, so a server your friends use needs a key.
- **`key`**: your own Anthropic API key. It is asked for with hidden input (or read from `$ANTHROPIC_API_KEY`), checked with a call that spends no tokens, written only to `.env` (which git ignores), and never printed.
- **`none`**: reports show the computed numbers without the written commentary. Everything else works.

At the end the script prints the worker's `ai: enabled (...)` or `ai: disabled (<reason>)` line.

## Useful flags

- `--yes`: never ask (for CI). Without `--email` it invites `player@rematch.test`.
- `--web-port=N`, `--mailpit-port=N`, `--postgres-port=N`, `--redis-port=N`: move a port that is taken. Sign-in links follow the web port. `--no-ports` publishes none.
- `--project=NAME`: a second copy beside the first.
- `sh scripts/setup.sh doctor`: re-check everything and explain each failure.

## Windows, macOS and Linux

- **Windows:** run the script in Git Bash (it comes with Git for Windows), not PowerShell or cmd. Docker Desktop runs on WSL 2: set its memory in `%UserProfile%\.wslconfig` (`[wsl2]` then `memory=12GB`) and run `wsl --shutdown` once.
- **macOS:** Docker Desktop, Settings, Resources, Memory: 12 GB. On Apple Silicon the script sets `DOCKER_DEFAULT_PLATFORM=linux/amd64`, because Stockfish is pinned as an x86-64 build; it runs under emulation, more slowly. This path is not yet tested on Apple hardware, so reports are welcome.
- **Linux:** Docker Engine and the Compose plugin. Add yourself to the `docker` group so the script runs without `sudo`.

## Troubleshooting

TROUBLESHOOTING_TABLE

## Stop or reset

```sh
docker compose down        # stop, keep your data
docker compose down -v     # stop and delete the database
sh scripts/setup.sh ai-stop
```

## Next

- [What Rematch does](/rematch/docs/features/), feature by feature.
- [Architecture](/rematch/docs/architecture/), if you want to change it.
- [Contributing](/rematch/docs/contributing/): the tests, the gates and how a change gets merged.
