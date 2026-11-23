---
title: Getting started
description: Run Rematch on your own machine with Docker, sign in and analyse your chess.com games.
sidebar:
  order: 1
---

Rematch is a beta (version 0.x) that you run yourself. There is no public sign-up yet. This page gets the whole app running on your machine and your chess.com games analysed.

## What you need

- **Docker** with about 16 GB of memory for the full stack (Docker Desktop on Windows or macOS, with around 12 GB assigned to it), and 20 GB of free disk.
- **Git.**
- Node 22 and pnpm 12 only if you want to work on the code. Running the app needs Docker alone.

## Five commands

```sh
git clone https://github.com/Terkea/rematch.git && cd rematch
cp .env.example .env
sh scripts/rust-cache-init.sh
docker compose up -d --build
docker compose ps
```

1. `.env.example` turns on the local host ports (the app on `3100`, the sign-in mailbox on `48025`).
2. `rust-cache-init.sh` creates the shared Rust build caches once per Docker host. Without them Compose stops with `external volume "ci-…" not found`.
3. The first build takes a while: it compiles the Rust worker and installs the web app. Later starts are cached.
4. `docker compose ps` should show `web` as healthy.

### Let yourself in

Sign-up is invite-only by default (`INVITE_ONLY=1`). Before you sign in, put your address in `.env` and restart the web app:

```sh
INVITE_ALLOWLIST=you@example.com
```

```sh
docker compose up -d web
```

## Sign in and link chess.com

1. Open <http://localhost:3100> and ask for a sign-in link.
2. The email goes to Mailpit, a local mailbox, at <http://localhost:48025>. Nothing leaves your machine.
3. Link your chess.com username. To show the account is yours, Rematch gives you a short code to put anywhere in your chess.com profile's location field. Unverified links see the analysis only, not the coach.
4. Your games import, and the worker analyses every move with Stockfish 19. The overview fills in as games finish; nothing waits for the whole history.

For a local trial you can skip the location code: `ALLOW_UNVERIFIED_LINKS=1` in `.env` treats your link as verified. Production refuses this setting.

## Optional pieces

- **The AI coach.** Add an `ANTHROPIC_API_KEY` to `.env` and restart. The worker logs `ai: enabled` or `ai: disabled (<reason>)`. Without a key, reports show the computed numbers without commentary, and everything else works.
- **Peer comparisons** need a rating-band reference built from a month of the open Lichess game database. Until one is built, weakness rows say "Not compared yet". The operator command is `lichess-db build --month YYYY-MM --band-reference`, described in [CLAUDE.md](https://github.com/Terkea/rematch/blob/main/CLAUDE.md).
- **Endgame tables.** The 3-4-5 piece Syzygy tables (about 940 MB) go in `data/syzygy`. Without them, analysis goes on without the tablebase probe.
- **Maia** (how hard a move is to find at your rating) is opt-in: add `docker-compose.maia.yml` to `COMPOSE_FILE`.

## Stop or reset

```sh
# Stop, and keep your data:
docker compose down
# Stop, and delete the database:
docker compose down -v
```

## Next

- [What Rematch does](/rematch/docs/features/), feature by feature.
- [Architecture](/rematch/docs/architecture/), if you want to change it.
- [Contributing](/rematch/docs/contributing/): the tests, the gates and how a change gets merged.
