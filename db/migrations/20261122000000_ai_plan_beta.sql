-- M5b Task B, ruling G1 (ADR 0075): the private beta's AI plan. The worker gives every account
-- the beta plan while the deployment is a beta (WORKER_DEFAULT_PLAN), so report_runs.plan
-- records it. Postgres 12+ allows ADD VALUE in a transaction; the value is not used here.
ALTER TYPE ai_plan ADD VALUE IF NOT EXISTS 'beta';
