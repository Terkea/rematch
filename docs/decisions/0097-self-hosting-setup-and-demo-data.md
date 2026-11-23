# 0097. Self-hosting: one setup script, its data, the AI choice and a demo player

Date: 2026-11-23. Status: accepted (lane `selfhost`, the owner's open-source launch plan, Lane L4).

## Context

Once the repository is public, strangers must be able to run the whole project, data included.
A fresh clone that followed the old `CONTRIBUTING.md` on Windows 11 (Git Bash, Docker Desktop
29.1, Compose 5.0) hit these snags:

- `nvm use` fails under nvm-windows, which does not read `.nvmrc`.
- `cp .env.example .env` publishes fixed host ports, and a second stack fails with `Bind for
  127.0.0.1:48025 failed: port is already allocated`.
- A plain `docker compose up -d --build` starts every service, including `rust-dev` and the old
  TypeScript `worker`.
- On a Docker host without the shared Rust caches, that `up` stops with `external volume
  "ci-cargo-registry" not found`: the cache script came after it (reproduced in a scratch
  project, because the dry-run host already had the volumes).
- The sign-in page says "Check your email", but no mail reaches Mailpit: sign-up is invite-only
  and `INVITE_ALLOWLIST` is empty.
- Nothing mentions the Syzygy tables, the Lichess reference or Maia, and every page stays empty
  until a real chess.com account is imported and analysed.

The first full-mode run of the new script found two more: `lichess-db syzygy --dest /data/syzygy`
wrote to `C:/Program Files/Git/data/syzygy` inside the container, because Git Bash rewrote the
argument, and the 2013-01 month cannot build a band reference (below).

## Decision

1. **`scripts/setup.sh [minimal|full|configure|doctor|data|ai|ai-start|ai-stop|ai-status]`**, POSIX
   sh (Git Bash, macOS, Linux), idempotent, never `sudo`: it says what to install. `--yes` (or no
   terminal) takes the defaults, for CI.
   - `.env` is merged from `.env.example`: missing keys are appended, no existing line changes.
     Local defaults are set only where empty: `ALLOW_UNVERIFIED_LINKS=1`, the user's email in
     `INVITE_ALLOWLIST` (`--email`, the prompt, or `player@rematch.test` when neither), a random
     64-hex `BETTER_AUTH_SECRET`, `COMPOSE_PATH_SEPARATOR=:`, and `COMPOSE_FILE` with the ports
     file plus the Maia and local-Claude overlays as chosen (each added once). Values travel to
     `awk` through the environment, never argv.
   - Ports are configurable (`WEB_PORT`, `MAILPIT_PORT`, `POSTGRES_PORT`, `REDIS_PORT`, or
     `--no-ports`); the web's `BETTER_AUTH_URL` follows `WEB_PORT`, so sign-in links do. The e2e
     overlay pins it to 3100, where the qa browser reaches web.
   - Checks: Docker 24+, Compose 2.24+ (the local-Claude overlay uses `!override`), Docker
     memory (12 GiB recommended, a warning only), free disk per mode, Node 22 and pnpm 12 (needed
     only on the host for `--ai=local`), and free ports. `doctor` re-checks everything, the
     services, the worker's `ai:` line and the data, and explains each failure.
   - Start: `rust-cache-init.sh` first, then `up -d --build --wait` of the services the app
     needs only (postgres, redis, mailpit, web, rust-worker, and ai-sidecar unless local), then a
     wait for the worker's migrations.
2. **Data.** A Compose `data` service (profile `data`) runs the worker image's `lichess-db` with
   `data/` writable.
   - Syzygy 3-4-5 (290 files, 984 MB, 101 s on the dry run): `lichess-db syzygy`, each file
     verified against the pinned `syzygy-345.sha256` and resumable per file (ADR-R2-22). `dc()`
     sets `MSYS_NO_PATHCONV=1` for Docker only and passes the checkout as `pwd -W`; the `data`
     service runs as the host user on Linux.
   - A Lichess month: `lichess-db build`, streamed and verified against Lichess's
     `sha256sums.txt` (ADR-R2-9). The binary already samples (`--max-games`,
     `--expected-games`), so no Rust change was needed. The default month is 2013-01 (18 MB,
     121,332 games), kept whole: it built the opening explorer in about 40 s on the dry run
     (59,585 rows). It cannot build the band reference: the reference filter requires `[%clk]`
     comments, which Lichess dumps carry from 2017-04 on, and the dry run failed with `no reference
     player qualified in any cell`. So the script adds `--band-reference` only for a month from
     2017-04 on (2017-04 is 3.5 GB and 11.3M games; 2026-10 is 31 GB and 95.4M games) and says so
     otherwise. It prints the size and game count (from `counts.txt`) first and skips a kind that
     is already built for that month. A stream cannot resume mid-file; a stopped build starts the
     month again.
   - Maia is opt-in in full mode (`--maia`): the overlay and its image (the weights are
     downloaded and SHA-256-checked by `maia2` at build time).
   - Minimal mode downloads none of these; the app works without them (no tablebase probe,
     "Not compared yet", difficulty pending).
3. **The AI choice** (menu, or `--ai=local|key|none`):
   - `local`: requires `claude auth status` to report `loggedIn`, else explains `/login`; adds the
     local-Claude overlay; starts `pnpm --filter @ci/ai-sidecar start:local` in the background with
     a pidfile (`.cache/ai-sidecar.pid`, log beside it) and waits for `/health` to report
     `auth: "local"`. One line says it is for the user's own local use only (ADR 0062 Amendment 2).
     On Git Bash `ai-stop` ends the native process tree with `taskkill /T`.
   - `key`: read with `stty -echo` (or from `$ANTHROPIC_API_KEY`, or stdin), checked with a
     models-list call (no tokens), the key handed to curl in a private header file, written only to
     `.env`, never printed. A rejected key writes nothing.
   - `none`: computed reports only; clears a stored key. `setup.sh ai` switches later.
4. **A demo player, `pnpm seed:demo`** (`scripts/seed-demo.sh` runs it in the web container, so it
   needs no host port or host Node). It writes a fictional player's analysed games, profile,
   example report, weekly review, strengths and trainer positions, built from the synthetic worlds
   of the e2e seeds and their test writers (public fixture games with fictional names). It is
   recognised by its username `demo.player`: neither chess.com nor Lichess allows a dot, so no
   real account can collide. It touches only its own rows (the dev database may hold real
   accounts), re-seeds idempotently (fixed ids, dates moved by whole weeks so the newest game is
   from the last seven days), and `--remove` deletes it (the user's login stays). It refuses
   production and any non-local database host (`assertLocal`).
   - The header shows a small "Demo data" label (`bg-well`, `text-ink-soft`, tokens only).
   - Neither the web's "Sync now" nor the worker's recurring sync (`DEMO_USERNAME_PREFIX` in
     `analysis::jobs::sync`) ever asks a platform for it.
5. **CI proof.** `.github/workflows/self-host.yml` (weekly and manual, read-only, actions pinned by
   SHA) runs `setup.sh minimal --yes --ai=none`, `seed:demo` twice, `doctor`, and a Playwright
   smoke (`playwright.selfhost.config.ts`, in the qa container with `docker-compose.smoke.yml`):
   sign in through Mailpit, the overview with the demo label, the trainer, the report. The shell
   tests (`scripts/test-setup.sh`, no Docker) also run in the CI `ts` job. Full mode is too heavy
   for CI and is tested by hand.

## Consequences

- One command from clone to a signed-in app with content; the docs (CONTRIBUTING, Getting
  started) describe minimal and full, the data table, the AI options and the snags.
- A new `.env.example` key reaches existing checkouts on the next `setup.sh` run.
- The demo's dates move with the calendar; its screenshots are not baselines.
- Re-seeding resets the demo's trainer progress (it is demo data).
