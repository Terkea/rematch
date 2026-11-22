-- R3 Task 18: idempotent metering and the SDK's cost estimate on ai_usage.

-- Idempotent metering (the semantics of R3 Task 25's migration 20261121030300, which adds the
-- same column and index): one row per paid call. A job keys each call (the report job by run,
-- claim, section and call; the coach chat by message, attempt and call), so a retried or re-run
-- write never meters a call twice. IF NOT EXISTS keeps this a no-op where 030300 ran first; at
-- the T25 merge, 030300's two statements must get IF NOT EXISTS too (see task-18-report.md).
ALTER TABLE ai_usage ADD COLUMN IF NOT EXISTS idempotency_key text;
CREATE UNIQUE INDEX IF NOT EXISTS ai_usage_idempotency_uq ON ai_usage (idempotency_key)
  WHERE idempotency_key IS NOT NULL;

-- ADR-R3-6 controller note: the Agent SDK's own cost estimate per metered call, in micro-USD
-- rounded half up, stored for comparison only. Budgets never sum it; they read cost_micro_usd,
-- which is ours (PricingTable).
ALTER TABLE ai_usage ADD COLUMN sdk_estimate_micro_usd bigint;
