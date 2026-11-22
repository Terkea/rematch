# ADR-R2-15. Fair scheduling and preview

Status: accepted (fair dequeue, Task 27). Task 28 appends the preview section.

## Context

Analyse jobs share one Postgres queue (ADR 0032). With R1's claim (`priority, run_at, id`), one account that imports 5 000 games holds the workers for hours, and a newcomer waits behind it. Spec §8d.1 asks for a round robin per account within each tier. R-12 fixes the order.

## Decision: fair round robin (`scheduler::fair::FairRoundRobin`)

**Order.** Claims go in the order `(tier, slice, joined_at, priority, run_at, id)`:
- **tier** = `priority / 2`. This is `job_priority`'s base, so the lichess-eval bit never changes the tier. A higher tier always goes first, whatever the slice.
- **slice** = `(served + rank − 1) / 5`.
  - `served` is the number of the account's games whose `analysis_started_at` falls in the last hour.
  - `rank` is the job's position, counting from 1, among the account's due jobs in `(priority, run_at, id)` order.
  - So every account gets 5 jobs per turn. A newcomer gets its first 10 before a heavy account gets its 11th in the hour.
- **joined_at** = `linked_accounts.created_at`. Within a slice, accounts that joined earlier go first.

**served comes from games, not jobs.** Finished jobs are pruned (R1 retention), so `served` is read from `games.analysis_started_at`. The analyse handler sets that column when it starts. Task 3 added `games_started_account_idx (analysis_started_at, account_id)`, so this read is an index range over the last hour.

**Analyse kinds only (fix round 1).** For a kind whose handler never sets `analysis_started_at`, `served` stays 0 and the slices never advance, so the order is not fair. The typed allow-list `ServedKind` (today only `analyse-game`) names the kinds that may be claimed fairly. `FairRoundRobin::for_kinds` refuses any other kind with `NotAServedKind`, and `dequeue` asked for a kind outside `FairRoundRobin::kinds()` returns `QueueError::InvalidOptions` and claims nothing. Add a variant only for a kind whose handler sets the column at start.

**The reference pseudo-user.** Jobs with `user_key NULL` (reference games, priority 200) count as one pseudo-user with `joined_at = infinity`. A job whose `user_key` has no `linked_accounts` row (a ghost account) is not folded into that pseudo-user: it keeps its own round robin, also with `joined_at = infinity`.

**Lock after ordering.** `FOR UPDATE` cannot share a query level with window functions. The statement therefore ranks first and locks the result. It is one statement:
1. It ranks the candidates and keeps the best `4 × limit`.
2. It locks them with `ORDER BY … LIMIT limit FOR UPDATE OF j SKIP LOCKED`, so it locks at most `limit` rows. The 4× headroom lets about four concurrent claimers fill their batches.
   - **One retry (fix round 1).** With more than four claimers, the others can hold every candidate, and a claim would come back empty while due work remains. The statement also returns how many candidates it considered. When a claim gets fewer than `limit` jobs although it had more candidates than it got, it is re-run once, in a new transaction (fresh snapshot), for the missing jobs over a wider window of `2 × 4 × limit + 32` candidates. One retry, not a loop: an empty result after it means the work really is held or gone.
3. Postgres re-checks `state = 'queued'` on any row a concurrent claimer has already taken, and skips it.
4. R1's claim update follows: the attempt is counted at claim, and the lease token is fresh.

### Staying index-backed (R1 queue rulings preserved)

R1 found that `kind = ANY($1)` sorts every ready row on each claim. The brief's literal statement had the same problem: a window over every ready row. The implemented statement instead runs in two phases on `jobs_user_ready_idx (kind, user_key, priority, run_at, id)` (migration `20261121020100_jobs_user_ready_id`, fix round 1, adds `id`):

1. **Heads.** A recursive skip scan finds each account with queued jobs. One `LIMIT 1` probe per account then reads its head, which is its first due job. A head has rank 1, so its key is exact. With several kinds, an account's head is its best head across kinds. Only the best `4 × limit` accounts by head key go on. This is safe because an account's own jobs never come before its head, so an account whose head misses the cut has no job in the global top `4 × limit` either.
2. **Expand.** For those accounts, the statement reads the first `4 × limit` due jobs per kind, as index ranges in order. It then ranks each account's jobs (`row_number`) and orders the result.

Two details of the index reads:
- They order by exactly the index columns `(user_key, priority, run_at, id)`.
  - With `user_key` leading, the planner walks the user index rather than filtering `jobs_ready_idx`. For the reference pseudo-user, filtering would walk every ready job of the kind.
  - `id` is the tie-break. Jobs enqueued in one transaction share `(priority, run_at)`. Before fix round 1 the index had no `id`, so a tie came back in index (heap) order and a `LIMIT` could skip the smallest id. Ordering by `id` without it in the index would need an incremental sort over the whole tie group (for example 2 000 reference games). With `id` as the last index column, the index order is the full claim order and no sort is needed. The regression test is `equal_priority_and_run_at_claims_the_smallest_id`.
- The cost is about `2 × accounts + kinds × (4 × limit)²` rows read from `jobs`, whatever the backlog.

Measured with 50 000 queued jobs over 500 accounts and `limit 16`: 5 145 rows read, about 25 ms, in both the custom and the generic plan. The plan guard `fair_claim_plan_is_index_backed` asserts the named indexes, no Seq Scan, and the row bound.

The other R1 rulings are unchanged:
- **The due rule.** It is still `run_at <= now() OR run_at <= created_at`, so the claim is immune to clock steps.
- **Re-queues.** Zero-delay re-queues keep their `run_at` (`FAIL_SQL` is untouched), and so keep their place in their account's order.
- **The reaper.** The two-pass reaper with `stall_grace` is untouched.
- **R1's own claim is untouched.** `PriorityFifo` (`Queue::dequeue`, the single-kind index-ordered claim) stays the default `RunnerConfig.strategy`. The fair strategy is opt-in per runner.

### Hook

`queue::strategy::SelectStrategy` has two methods: `dequeue(queue, kinds, owner, limit, lease)` and `order_by()`. The second is used for `Debug` and logs.

`Queue::claim_with` runs any custom claim statement under the queue's statement timeout and backstop, and returns `Claimed { jobs, candidates }` (the statement returns one row per claimed job, or one row of NULL job columns, each with the candidate count). `scheduler::fair::fair_order_sql()` is the ranking without the lock (a CTE named `fair`). Its parameters are `$1` kinds, `$2` the candidate count (the first `$2` rows of `fair` are exact), `$3` the served window in ms and `$4` the slice length. Task 30 (ETA) shares it, and passes a `$2` as deep as the ETA it needs.

## Consequences

- **Lag in `served`.** A job that is claimed but not yet started is no longer queued, and is not yet in `served`. Until the handler sets `analysis_started_at`, the account's next job ranks one place too early. With W jobs in flight, an account can be ahead by at most W. The runner test bounds the spread at 5 plus the in-flight count.
- **Cost grows with accounts (scale accepted to about 10 000).** The claim is linear in the number of accounts that have queued work: about 2 index probes, roughly 12 µs, per account. That is about 25 ms at 500 accounts and about 120 ms at 10 000. The trigger for a maintained per-account head table is **5 000 accounts with queued work**. The release-only guard `fair_claim_at_2000_accounts_stays_linear` (ignored in debug builds, since it is a wall-clock bound) asserts a claim at 2 000 accounts stays under 72 ms (3 × 2 000 × 12 µs).
- **Ties.** Exact ties in `(priority, run_at)` are claimed by `id` (see above).
- **Contention.** Beyond four concurrent claimers, the one retry covers the window. `contended_claimers_never_come_back_empty_while_work_remains` (8 claimers, limit 1) and `short_claim_retries_past_rows_locked_by_others` guard it.

## Preview pass

Task 28 (`scheduler::preview`). A quick first look at 30 000 nodes, MultiPV 1 (`PREVIEW_NODES`, protocol `sf19-n30k-preview-v1`), so a user sees early results while the full pass is queued.

- **The only exception to one protocol.** Preview evals are never read from or written to the eval cache, and never touch `position_evals`, `moves`, `game_metrics`, `weakness_series_point` or `games.analysis_status`. Each distinct `fen_key` of a game is searched once; terminal positions use the R1 fixed scores.
- **Stored in `game_previews`** (one row per game, `ON CONFLICT (game_id) DO UPDATE`): both accuracies and `moves = [{ply, judgement, deltaW, accuracy}]`. The write is skipped if the game became `done` while the preview ran.
- **Clearly a preview.** The event is `game.analysed {gameId, accuracy, preview: true}`. A preview is skipped outright when the game is already `done`.
- **Never used for KPIs, posteriors or the timeline.** Only the full pass feeds them.
- **Superseded by the full pass,** which deletes the row on completion (Task 31).
- **Queue.** `enqueue_previews`: dedupe `preview:<game>`, priority 0, `user_key` = account, 2 attempts. Preview jobs are not an analyse kind, so they stay on the default priority-FIFO claim (the fair round-robin is analyse-only, T27). Metrics: `job_metrics` kind `preview-game`, stages `parse_ms`, `engine_wall_ms`, `persist_ms`.
