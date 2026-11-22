# ADR-R2-17. ETA and autoscaling hooks

Status: accepted (Task 30, spec sections 8d.4 and 8d.5).

## Context

An account that imports thousands of games waits behind the others. The UI needs an honest "how long", and the operator needs a signal for adding servers. No cloud API is in R2 scope.

## Decision: the ETA (`scheduler::eta`)

**Measurements.**
- `total` = the account's games with `selected_at` set and status `pending`, `analysing` or `done`. `done` = those `done`. Failed and skipped games are not counted.
- `games_per_sec` = `job_metrics` rows of kind `analyse-game` in the last 10 minutes, divided by 600. With fewer than 5 rows it is `None`.
- `queued_ahead` = the 1-based position, in the fair order of ADR-R2-15 (`fair_order_sql`, no lock), of the account's last queued analyse job.
- `eta = queued_ahead / games_per_sec`, one figure, never a range.

**Honesty rules.**
- Not enough throughput data: `eta` is `None` and the sentence is `analysing {done} / {total}`. There is no spinner and no default rate.
- `fair_order_sql` is exact only for its first `$2` rows (ADR-R2-15). The ETA reads a window of 400 places (`ETA_WINDOW`). If any queued job of the account is not placed inside it, or is delayed (not due, so not in the order), the last position is unknown and `eta` is `None`. `queued_ahead` is then a lower bound.
- Nothing queued is a known zero (`Some(0)`, "under 1 min") only when every selected game is done. Games that are pending or running without a queued job (not scheduled yet, or all in flight) give `None`.
- One delayed job (retry back-off) makes the whole ETA `None`, even if the last due job has an exact position. This is conservative and can be noisy for accounts with retries.
- No NaN or infinity: the rate is a count over a constant, and the duration conversion is checked.
- The sentence is `about {m} min, analysing d / t` with `m = ceil(eta / 60 s)`, or `under 1 min, ...` below 60 s.

**Plan guards.** The count and rate reads, and the queue-lag read, have `EXPLAIN` tests (custom and generic plans, no Seq Scan) on seeded tables. The rate read needed `job_metrics_kind_created_idx` (Task 3).

## Decision: queue lag and autoscaling hooks (`scheduler::lag`)

- `queue_lag`: `oldest_p0_age` is the age of the oldest due queued job with `priority / 2 <= 2` (analyse and preview), counted from the later of `run_at` and `created_at`; a delayed job does not lag. `queued_games` counts queued analyse jobs. `core_seconds_queued` = queued games x the mean `engine_cpu_ms` of the last 24 h (12 000 ms when there is no data), divided by 1000.
- `scale_advice` is a pure function and is the hook for an external autoscaler: when P0 lag is over `p0_lag_threshold` (3 min) it adds `min(max - current, max(1, ceil(core_seconds / (engines_per_server x 600))))` servers, as far as the monthly cap pays for whole server-hours. When idle for 15 min or more with extra servers running, it removes one. Otherwise it holds.
- Spend needs a positive, finite owner-approved cap AND a positive, finite price, and the projected spend must stay within the cap. A zero, negative, NaN or missing cap or price always means `Hold`, so the default config (both 0) never adds a server. A NaN spend also holds. A bad backlog figure asks for one server, not many. `engines_per_server == 0` is a misconfiguration and holds.
- R2 has no cloud API. Whoever runs the autoscaler supplies `idle_for`, `current_extra` and the month's spend, and applies the advice.
