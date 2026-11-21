-- R2 Task 27 fix round 1 (ADR-R2-15): the fair claim reads each account's jobs from
-- jobs_user_ready_idx in claim order (priority, run_at, id). Without id, equal (priority, run_at)
-- ties came back in heap order, so a LIMIT could skip the smallest id. Adding id makes the index
-- order the full claim order. The column list is the only change (same name, same predicate).
DROP INDEX jobs_user_ready_idx;
CREATE INDEX jobs_user_ready_idx ON jobs (kind, user_key, priority, run_at, id)
  WHERE state = 'queued';
