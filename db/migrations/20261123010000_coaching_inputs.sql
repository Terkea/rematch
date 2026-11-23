-- Coaching-input snapshots (ADR 0086, finding Q5): every build-profile stores the CoachingInput
-- of the profile it just wrote, so the coach reads an input as current as the profile instead of
-- the newest report's (reports are throttled; a report input can be a day old or an older
-- contract version). A snapshot is immutable; old ones are pruned, except any a stored chat
-- answer cites.
CREATE TABLE coaching_inputs (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  -- `input.inputVersion` ('coaching-input-v1' or 'coaching-input-v2').
  input_version text NOT NULL,
  -- `input.generatedAt`: the build's clock.
  generated_at timestamptz NOT NULL,
  input jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX coaching_inputs_latest_idx
  ON coaching_inputs (account_id, time_class, created_at DESC);

-- Which input a chat answer was written from: the snapshot (when it was one) and the sha256 of
-- the input's canonical JSON (equal to `report_runs.input_hash` when it was a report's).
ALTER TABLE coach_messages
  ADD COLUMN coaching_input_id uuid REFERENCES coaching_inputs(id) ON DELETE SET NULL,
  ADD COLUMN input_hash text;
-- The ON DELETE SET NULL and the pruning look messages up by it.
CREATE INDEX coach_messages_coaching_input_idx
  ON coach_messages (coaching_input_id) WHERE coaching_input_id IS NOT NULL;
