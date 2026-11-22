# ADR-R2-25. Worker wiring of the profile engine, and the read-only CLI

Status: accepted (R2 Task 35). Placeholder number; the controller renumbers it with the other R2
records.

## Context

R2 Tasks 1-34 built the profile engine as libraries: the enrichment (Tasks 23-25), the analyse
hook (Task 31, ADR-R2-24), the reference jobs (Task 32, ADR-R2-11), build-profile (Task 33) and the
CoachingInput builder (Task 34, ADR-R2-18). Nothing ran them. Several hand-offs named this task:
pass the tablebase probe into the enrichment, set the hook, register the R2 job kinds, schedule the
sweeps and requeue unenriched games, move the build-profile key release behind a queue API, fill
the protocols of band-reference builds, and expose the profile on the command line.

## Decision

### `start_workers`

- **Environment.** `WorkerEnv` gains `DATA_DIR`, `MAIA_URL` and `WORKER_DEFAULT_PLAN`
  (`beta`, `free` or `pro`). The default plan is **`beta`**, not the brief's `free`: the T29 ruling
  (ADR 0041, ADR-R2-16) makes the private beta, with no plan limit, the default for every account.
  `ScheduleDeps.default_plan` applies it to schedule-analysis payloads that name no plan; an
  explicit plan still wins.
- **Enrichment.** One `EnrichDeps` shape for the hook and the reference jobs: the main engine
  pool, `DbEnrichmentStore`, the `CachedTbProbe` when `SYZYGY_PATH` gave one, and the Maia
  estimator only when `MAIA_URL` is set. Without Maia, difficulty is `pending_maia` and the sweep
  answers it later; it is never estimated (owner override).
- **Hook.** `AnalyseGameDeps.hook = ProfileHook`, so every analysis writes its move features,
  metrics, opening and series rows in the done transaction and enqueues `build-profile`.
- **Runners** (kind: concurrency). Only `analyse-game` claims with the fair round robin
  (ADR-R2-15); every other kind keeps `PriorityFifo`.

  | Kind | Concurrency | Hard timeout |
  |---|---|---|
  | import-account | 2 | none |
  | analyse-game | `pool_size` (fair) | 11 min |
  | schedule-analysis | 2 | 2 min |
  | preview-game | `max(1, pool_size / 2)` | 5 min |
  | build-profile | 2 | 10 min |
  | analyse-reference-game | 1 | 11 min |
  | difficulty-sweep | 1 | 10 min |
  | reference-difficulty-sweep | 1 | 10 min |
  | reconcile (optional) | 1 | 5 min |

  The brief said the reference runner shares the analyse pool and the fair order puts it last.
  The round robin cannot claim reference jobs (its allow-list counts `games.analysis_started_at`,
  which reference games do not set), so reference games get their own FIFO runner on the same
  engine pool. Its concurrency is 1 (four engine requests per game), so user analyses keep most of
  the engines while a band-reference build runs. Raising it is a one-line change once a build's
  wall time matters more than user latency.
- **Start-up maintenance.** The opening dataset is warmed off the async threads; the two
  recurring sweeps are scheduled (deduped); `requeue_unenriched` runs once. A failed requeue is
  logged and the worker still starts; the next start retries it.
- **Connections.** `max_connections = pool_size + preview + 24`. The module docs list what the 24
  cover.
- **Introspection.** `RunningWorkers.claims` records each runner's kind, concurrency and claim.
  `maia_configured` records whether `MAIA_URL` was set. Tests and the ready log line use both.

### Queue and reference builds

- `Queue::release_dedupe_key(&Job)` replaces the SQL that build-profile ran on `jobs` (T33
  hand-off, ADR-R2-18 step 7). It is fenced by the lease token and `state = 'running'`.
- At activation, the band-reference finaliser fills NULL `protocol` and `enrichment_protocol` with
  `sf19-n300k-v1` and `enr-v1` (T21 hand-off), in the activation transaction.
- `lichess-db resume [--build <id>]` re-enqueues the jobs of every verified `building`
  band-reference build (`resume_all_reference_enqueues`; idempotent). The worker does not schedule
  it: `worker` does not depend on `lichess-db`, and only the coordinator edits `Cargo.toml`. The
  enqueue that can crash runs in the `lichess-db` process, so the operator reruns `resume` (or
  `build`) after a crash. Scheduling it in the worker needs a coordinator edit (see the task
  report).

### CLI

- `cli profile --platform <p> --username <u> --time-class <tc> [--json]`. It prints
  `band B2  analysed 214/3912  focus-ready weaknesses N  areas: openings 54  ...`. An area shows its
  score, `need N more`, or `no benchmark`. A note line says so when no band-reference build is
  active. With `--json` it prints the whole validated CoachingInput.
- `cli eta --platform <p> --username <u>` prints `format_eta`.
- `cli queue-lag [--json]` prints `QueueLag` and `scale_advice`. The JSON keys are
  `oldestP0AgeSeconds` (null without a P0 job), `queuedGames`, `coreSecondsQueued`, and `advice`
  (`{action, servers}`). The advice uses the default `ScaleConfig`, whose zero cap and price never
  add a server.
- These commands only read. They need neither Stockfish nor a worker, and never create an
  account. `cli analyse` is unchanged (import, then schedule, then wait; T29).
- `cargo run -p coaching-input --example validate -- <file>` checks a document against the TS
  schema export and the contract's own rules.

### Runtime image and compose

- The image also carries `lichess-db`.
- `rust-worker` gets `DATA_DIR=/data`, `SYZYGY_PATH=/data/syzygy`, `MAIA_URL` and
  `WORKER_DEFAULT_PLAN`. It mounts `./data:/data:ro`, with the tables mount
  (`SYZYGY_HOST_DIR`) over `/data/syzygy`.
- `docker-compose.maia.yml` puts `rust-worker` on `maia_internal`, with
  `MAIA_URL=http://maia:8080`.

## Left open (parked, with reasons)

- **Events.** Build-profile events stay best effort. The web reloads profile state on page load,
  so no reconciliation is added (T33 ruling).
- **Lost follow-ups** (T33 M6). A crash between a game's commit and its `after_commit` loses one
  build-profile enqueue. The account's next analysed game, or the next build's follow-up check,
  rebuilds it. Games whose hook failed are requeued at start. A periodic reconcile is deferred
  until it is observed.
- **Crash leftovers of `lichess-db build`** (T20 m2). A safe reaper needs a lease or heartbeat
  column, which means a migration. Leftover `building` rows are never served. The operator fails
  one with SQL or reruns the build.
- **Capped builds** (T20). A capped explorer build still activates with a warning. A capped
  reference build never activates. `--allow-capped` stays a product decision. This task's smoke
  ran no dump build.
- **Double download** with `--explorer --band-reference` (ADR-R2-11). It is still two passes. A
  combined sink is a lane D change.

## Consequences

- One worker process runs the whole R2 pipeline. A `docker compose up rust-worker` analyses,
  enriches, builds profiles and serves the CLI.
- R3 Task 26 (AI worker wiring) edits the same `start_workers`. Its runners should go through the
  same `Spawned::spawn`, so `claims` stays complete, and its connections need adding to
  `POOL_HEADROOM`.
