# 0078. Honest coach availability, uncompared weaknesses and R2-backed examples

Date: 2026-11-22. Status: accepted (bug hunt on the owner's real data, batch A). Builds on
ADR 0062 (the sidecar), ADR 0068 (the weakness read model), ADR-R2-6 (timeline states) and
ADR 0060 (chat tools).

## Context

The bug hunt ran the app on real data with no rating-band reference built and, at first, no AI.
Three things claimed more than the data held:

- With no benchmark, Rust stores every weakness as `not_an_issue` with a null `excess` and band
  (ADR-R2-6: "no reference claims nothing"). The web showed that as "Under control", "Not an
  issue", "Nothing stands out" and "Now not an issue".
- The web could not tell whether the worker had an AI client, so it offered "Build my report",
  "Ask your first question" and "100 of 100 left" while nothing could answer.
- The coach's `find_examples` read only the report's exemplars. With nothing ranked there are
  none, so the coach said it "found none" while R2 had stored the occurrences.

## Decision

1. **Uncompared weaknesses are a read-model state, not a Rust state.** `WeaknessSummary.compared`
   is false when the stored `band_median` or `excess` is null (Rust writes `excess` only with a
   benchmark, `profile::estimates::estimate_row`). The timeline lane carries `compared: false`.
   Every surface then says "Not compared yet: no benchmark for players at your level yet" and
   still shows the weakness's own rate, range and n: the weaknesses list (its own group) and
   detail, the overview's "What to work on" (`LeadFinding.none.compared`), the report's "What's
   costing you games" (no ranked weakness and no metric band median) and every timeline. With a
   benchmark nothing changes. Rust is unchanged: a distinct state would ripple through the
   detector, the contract and the AI input for no gain, since the null `excess` already says it.
2. **The estimate's window is stated.** Rust counts only points at most `WINDOW_DAYS` (183) older
   than the build, so its counts read "in the last 6 months", and the detail's weekly list covers
   the same window (from `updated_at` minus 183 days, up to 28 ISO weeks), so the two add up.
3. **The worker records its AI state.** A worker with AI runners upserts one `ai_status` row at
   start (`enabled`, its `ai: enabled|disabled (...)` line, never a credential) and refreshes
   `heartbeat_at` every 30 s; a stopped worker's row goes stale after 120 s. The web reads
   `coachStatus`: `available` (a fresh enabled row), `no_ai` (fresh rows, none enabled) or
   `offline` (no fresh row), using the database clock. While not `available`:
   - the coach list, a new conversation and a thread say "Coach not available right now", keep
     past answers readable, disable the composer and show no allowance;
   - an empty report offers "Build it from your numbers" with `no_ai` (a computed-only report)
     and nothing with `offline`.
   The breaker opening at runtime is not reflected; the row states the startup choice.
4. **The AI disclosure sits only beside AI text** (each answer and section carries its own), so
   the coach list and an empty new conversation show none.
5. **`find_examples` reads R2's stored occurrences** after the report's exemplars, scoped to the
   conversation's account and time class: a weakness's per-game `exemplarPlies`
   (`game_metrics.metrics.weakness`, `W3` covering `W3.shallow` and `W3.deep`), a motif's erring
   player moves whose best or refuting line carries the tag (`move_features`, the contract's
   snake_case mapped to R2's camelCase), and an opening line's worst player move in its first 30
   plies, one per game (`game_openings`). A position's tags now include its stored line tags.
   An empty result is therefore a real "none found".
6. **The first question always opens its thread.** Once the server returns the thread id the new
   conversation page links to it and, if the client navigation has not left within 4 s, loads it
   with a full navigation.

## Consequences

- One small table and migration (`20261122010000_ai_status.sql`), regenerated into Drizzle.
- The e2e seed writes an enabled `ai_status` row for the simulated worker.
- Building the band reference (bug hunt B1) turns every "Not compared yet" back into a state.
