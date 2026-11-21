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

**The reference pseudo-user.** Jobs with `user_key NULL` (reference games, priority 200) count as one pseudo-user with `joined_at = infinity`. So do jobs whose account no longer exists.

**Lock after ordering.** `FOR UPDATE` cannot share a query level with window functions. The statement therefore ranks first and locks the result. It is one statement:
1. It ranks the candidates and keeps the best `4 × limit`.
2. It locks them with `ORDER BY … LIMIT limit FOR UPDATE OF j SKIP LOCKED`, so it locks at most `limit` rows. The 4× headroom lets about four concurrent claimers fill their batches.
3. Postgres re-checks `state = 'queued'` on any row a concurrent claimer has already taken, and skips it.
4. R1's claim update follows: the attempt is counted at claim, and the lease token is fresh.

### Staying index-backed (R1 queue rulings preserved)

R1 found that `kind = ANY($1)` sorts every ready row on each claim. The brief's literal statement had the same problem: a window over every ready row. The implemented statement instead runs in two phases on `jobs_user_ready_idx (kind, user_key, priority, run_at)`:

1. **Heads.** A recursive skip scan finds each account with queued jobs. One `LIMIT 1` probe per account then reads its head, which is its first due job. A head has rank 1, so its key is exact. With several kinds, an account's head is its best head across kinds. Only the best `4 × limit` accounts by head key go on. This is safe because an account's own jobs never come before its head, so an account whose head misses the cut has no job in the global top `4 × limit` either.
2. **Expand.** For those accounts, the statement reads the first `4 × limit` due jobs per kind, as index ranges in order. It then ranks each account's jobs (`row_number`) and orders the result.

Two details of the index reads:
- They order by exactly the index columns `(user_key, priority, run_at)`.
  - With `user_key` leading, the planner walks the user index rather than filtering `jobs_ready_idx`. For the reference pseudo-user, filtering would walk every ready job of the kind.
  - Leaving out `id` avoids an incremental sort that would read a whole tie group (for example 2 000 reference games enqueued in one transaction). Within an exact tie, candidates come in index order; the ranking and the claim still order by `id`.
- The cost is about `2 × accounts + kinds × (4 × limit)²` rows read from `jobs`, whatever the backlog.

Measured with 50 000 queued jobs over 500 accounts and `limit 16`: 5 145 rows read, about 25 ms, in both the custom and the generic plan. The plan guard `fair_claim_plan_is_index_backed` asserts the named indexes, no Seq Scan, and the row bound.

The other R1 rulings are unchanged:
- **The due rule.** It is still `run_at <= now() OR run_at <= created_at`, so the claim is immune to clock steps.
- **Re-queues.** Zero-delay re-queues keep their `run_at` (`FAIL_SQL` is untouched), and so keep their place in their account's order.
- **The reaper.** The two-pass reaper with `stall_grace` is untouched.
- **R1's own claim is untouched.** `PriorityFifo` (`Queue::dequeue`, the single-kind index-ordered claim) stays the default `RunnerConfig.strategy`. The fair strategy is opt-in per runner.

### Hook

`queue::strategy::SelectStrategy` has two methods: `dequeue(queue, kinds, owner, limit, lease)` and `order_by()`. The second is used for `Debug` and logs.

`Queue::claim_with` runs any custom claim statement under the queue's statement timeout and backstop. `scheduler::fair::fair_order_sql()` is the ranking without the lock (a CTE named `fair`). Task 30 (ETA) shares it.

## Consequences

- **Lag in `served`.** A job that is claimed but not yet started is no longer queued, and is not yet in `served`. Until the handler sets `analysis_started_at`, the account's next job ranks one place too early. With W jobs in flight, an account can be ahead by at most W. The runner test bounds the spread at 5 plus the in-flight count.
- **Cost grows with accounts.** The claim's cost grows with the number of accounts that have queued work: about 2 index probes per account. A very large population (10⁵ accounts with work) would need a maintained per-account head table.
- **Ties.** Exact ties in `(user_key, priority, run_at)` beyond `4 × limit` are taken in index order rather than strictly by `id`.
