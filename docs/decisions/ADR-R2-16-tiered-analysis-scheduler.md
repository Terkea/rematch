# ADR-R2-16. Tiered analysis scheduler

Status: accepted (selection rules, Task 26; the job, the `imported` status and the integration, Task 29).

## Selection rules (pure, `scheduler::select`)

Values are TS M2 Task 23 verbatim: free 1000/50/0/no P2/20 per week, pro 10 000/200/100/P2/300 per week; 15 per month, 36 months, 20 recent, 90 days, 10 plies, batch 25.

- **Primary time class.** The class with the most games in the last 90 days (ties to the longer class). A dormant account (nothing in 90 days) uses the last 365 days, then all time. P0 then takes the newest games regardless of date, and a dormant account has only its primary as active. Future-dated games (`played_at > now`) are ignored and logged.
- **P3 is rated only** (every new rated game created after the first scheduling run, within the weekly budget).
- **P0 before P3.** A new game goes to P0 while P0 has room (a free user with 30 analysed games sends new games to P0 until 50). This is the plan's literal behaviour, pinned by a test.
- **P2.** Calendar months are taken in UTC, up to and including 36 months back. The 15-per-month quota counts every analysed game of that month, whatever its tier, plus this run's picks. Rated games are taken first and spaced chronologically; casual games only fill what rated cannot. Picks are emitted newest first.
- **P2 for the primary time class runs in the same pass as P0** (P0 takes the newest 200, P2 up to 25 older). Kept on purpose; pinned by a test.
- **Equal `played_at`** is broken by game id.
- Candidates are `Pending` with no tier in `select_for_analysis`; the schedule job maps `imported` rows onto that (below).

## The `imported` status and the import (Task 29)

- The importer stores new games as `imported` with `ply_count` (SAN tokens of the main line: headers, comments, variations, NAGs, move numbers and the result are not moves). It enqueues no analyse job. After the run commits (the run row is `done`), it enqueues one `schedule-analysis` job, deduped on `schedule:<account>`; a failed enqueue retries the import, which enqueues again.
- A payload without `max` imports with `ImportAccountDeps::default_max = None`: no limit on the beta plan (below). With per-user limits it becomes `tier_limits(plan).import_max`. An explicit `max` (the CLI's `--max`) is R1's budget: one `max` shared by the passes of a run, new games first, and a pass cut off by `max` keeps its frontier, so a larger `max` later continues below it.
- R1 rows stay valid: a `pending` game with no tier was already enqueued by R1 and is never re-selected; the import no longer re-enqueues existing `pending` games (the reconciler does).

## The schedule-analysis job (`scheduler::schedule_job`)

- `plan` comes from the payload (`"beta"` or none = beta, `"free"`, `"pro"`) until per-user limits exist. Any other value is unrecoverable.
- **Beta plan (owner ruling, ADR 0041: private beta).** Until plans exist there is no plan limit. `Plan::Beta` is the default for every account and for the CLI. It selects every `imported` game (no 50-game P0 cap, no weekly P3 budget, no batch size, no stopping, no bullet or 36-month or length exclusion), bounded only by the import's `--max`. Each game gets the tier the plan rules would give it first (0 in the primary class, 3 for a new rated game in an active class, 1 in another active class, else 2) and pro's priority bases, and the analyse jobs run in that priority order. The free and pro rules are kept, tested, and unused by default. Pinned by `cli_max_500_on_500_imported_games_schedules_all_500`.
- One transaction: lock the account row, load every game of the account and its weakness estimates, select, then set the selected games `pending` with `analysis_tier`, `analysis_priority`, `selected_at` and `pending_since = now()`, fenced on `analysis_status = 'imported'`. Analyse jobs are enqueued after the commit at the stored priority (one `enqueue_many` batch, the same rows as `enqueue_analyse_with_priority`). A lost enqueue leaves `pending` games, which the reconciler re-enqueues at `coalesce(analysis_priority, 100)`.
- **Priority table** (`job_priority`, lower first): `BASE * 2 + (0 with Lichess evals, else 1)`, BASE tier 0 {pro 1, free 2}, tier 3 {pro 3, free 4}, tier 1 5, tier 2 10. Previews run at 0 and reference games at 200. R1 games keep 100.
- **Batch-wise adaptive stopping.** P1/P2 add at most 25 games per time class per run, and only when no tier-1/2 game of that class is still `pending` or `analysing`. A time class stops getting backfill once every rankable weakness estimate of it is precise (`is_precise`: width at most a quarter of the band median, or `n_unweighted` at least the key's minimum evidence). Non-weakness series (motifs, openings, endgames) are not precision targets.
- **P3 is a rolling 7-day window**: the budget is `p3_per_week` minus the tier-3 games whose `selected_at` is within the last 7 days.
- **"Created after the first run" is `played_at`.** `games` has no insertion time. A game played after the first run was necessarily imported after it, and an older game found by a later backfill never spends the new-games budget. `analysis_scheduled_at` is set on the account's first run only and is the `first_scheduled_at`.
- **Previews on the first run.** When `analysis_scheduled_at` was NULL, the job enqueues preview jobs (ADR-R2-15) for the account's first `PREVIEW_GAMES = 20` tier-0 games still to be analysed, read back from the database in analysis order (priority, rated first, newest first). `analysis_scheduled_at` is set only after that enqueue succeeds, so a failed preview enqueue leaves the account on its first run and the retry enqueues the same previews (`preview:<game>` dedupes a repeat). Later runs enqueue none.
- **No primary time class** (every game future-dated): nothing is selected and the markers are kept, so a later run reconsiders the games.

## No imported game is lost (reconciler, Task 29)

- `imported` games are never enqueued for analysis by the sweep.
- `pending_since` on an `imported` row means "waiting for the scheduler since": the column default sets it at insert, and every schedule run clears it on each `imported` game it read and did not select. A game committed after a run read the account (its import's schedule enqueue deduped onto that running job), or the games of an import that failed for good, keep it.
- Each sweep gives a `schedule-analysis` job to every account with such a game older than `pending_grace` (2 min) and no live import job (a running import enqueues it when it ends). The dedupe key makes this idempotent.
- Cost: migration `20261121020300` adds the partial index `games_imported_waiting_idx (account_id) WHERE analysis_status = 'imported' AND pending_since IS NOT NULL`. The sweep reads only the waiting rows (`SELECT DISTINCT account_id ...`), so its cost is the number of stragglers, not of imported games. Plan-guarded (custom and generic plans, 50k seen imported rows): the index is used and `games` is never seq-scanned.

## Worker wiring (Task 29)

- The worker runs a `schedule-analysis` runner (2 at once, 2 min hard timeout) and a `preview-game` runner (1 at once, 5 min hard timeout) next to import, analyse and reconcile. `POOL_HEADROOM` grows from 8 to 13 connections for them.
- Only the analyse runner claims with `FairRoundRobin` (ADR-R2-15): `analyse-game` is the only kind whose handler records `analysis_started_at`, which the round robin counts as served. Every other runner, including R3's `interactive-analyse`, keeps R1's `PriorityFifo`, set explicitly in each runner config. `fair_order_sql()` is exact only for the first `4 x limit` rows; callers that need a deeper order pass a larger limit (Task 30).
- The CLI's `analyse` command waits for import, schedule-analysis and analyse jobs, and does not wait for `imported` games: those were not selected and wait for a later run.
