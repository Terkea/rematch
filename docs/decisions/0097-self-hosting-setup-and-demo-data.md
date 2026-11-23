# 0097. Self-hosting: one setup script, its data, the AI choice and a demo player

Date: 2026-11-23. Status: accepted (lane `selfhost`, the owner's open-source launch plan, Lane L4).

## Context

Once the repository is public, strangers must be able to run the whole project, data included.
A fresh clone that followed the old `CONTRIBUTING.md` literally hit, in order: Compose stopping
with `external volume "ci-cargo-registry" not found` (the cache script came after `up`); a plain
`docker compose up -d` starting every service, including `rust-dev` and the legacy TS worker; no
way to sign in (invite-only with an empty `INVITE_ALLOWLIST`); a linked account showing analysis
only (`ALLOW_UNVERIFIED_LINKS` unset); and no word on the Syzygy tables, the Lichess reference or
Maia. Even with everything up, every page stayed empty until a real chess.com account had been
imported and analysed.

## Decision

1. **`scripts/setup.sh [minimal|full|configure|doctor|data|ai|ai-start|ai-stop|ai-status]`**, POSIX
   sh (Git Bash, macOS, Linux), idempotent, never `sudo`: it says what to install. `--yes` (or no
   terminal) takes the defaults, for CI.
   - **`.env`** is merged from `.env.example`: missing keys are appended, no existing line changes.
     Local defaults are set only where empty: `ALLOW_UNVERIFIED_LINKS=1`, the user's email in
     `INVITE_ALLOWLIST` (`--email`, the prompt, or `player@rematch.test` when neither), a random
     64-hex `BETTER_AUTH_SECRET`, `COMPOSE_PATH_SEPARATOR=:`, and `COMPOSE_FILE` with the ports
     file plus the Maia and local-Claude overlays as chosen (each added once). Values travel to
     `awk` through the environment, never argv.
   - **Ports are configurable** (`WEB_PORT`, `MAILPIT_PORT`, `POSTGRES_PORT`, `REDIS_PORT`, or
     `--no-ports`); the web's `BETTER_AUTH_URL` follows `WEB_PORT`, so sign-in links do. The e2e
     overlay pins it to 3100, where the qa browser reaches web.
   - **Checks:** Docker 24+, Compose 2.24+ (the local-Claude overlay uses `!override`), Docker
     memory (12 GiB recommended, a warning only), free disk per mode, Node 22 and pnpm 12 (needed
     only on the host for `--ai=local`), and free ports. `doctor` re-checks everything, the
     services, the worker's `ai:` line and the data, and explains each failure.
   - **Start:** `rust-cache-init.sh` first, then `up -d --build --wait` of the services the app
     needs only (postgres, redis, mailpit, web, rust-worker, and ai-sidecar unless local), then a
     wait for the worker's migrations.
2. **Data.** A Compose `data` service (profile `data`) runs the worker image's `lichess-db` with
   `data/` writable.
   - **Syzygy 3-4-5** (290 files, 984 MB): `lichess-db syzygy`, each file verified against the
     pinned `syzygy-345.sha256` and resumable per file (ADR-R2-22).
   - **A Lichess month** for the explorer and the band reference: `lichess-db build --explorer
     --band-reference`, streamed and verified against Lichess's `sha256sums.txt` (ADR-R2-9). The
     binary already samples (`--max-games`, `--expected-games`), so no Rust change was needed. The
     default month is **2013-01** (18 MB, 121,332 games), kept whole; a recent month is about
     31 GB and takes hours. The script prints the size and game count (from `counts.txt`) first
     and skips a month that already has a build. A stream cannot resume mid-file; a stopped build
     re-runs from the start, which for the default month is a minute. The trade-off, documented:
     a 2013 month has fewer players per rating band, so peer comparisons are coarser.
   - **Maia** is opt-in in full mode (`--maia`): the overlay and its image (the weights are
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
   recognised by its username **`demo.player`**: neither chess.com nor Lichess allows a dot, so no
   real account can collide. It touches only its own rows (the dev database may hold real
   accounts), re-seeds idempotently (fixed ids, dates moved by whole weeks so the newest game is
   from the last seven days), and `--remove` deletes it (the user's login stays). It refuses
   production and any non-local database host (`assertLocal`).
   - The header shows a small **"Demo data"** label (`bg-well`, `text-ink-soft`, tokens only).
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
