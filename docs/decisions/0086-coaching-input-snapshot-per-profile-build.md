# 0086. The coach reads a coaching-input snapshot written at every profile build

Date: 2026-11-23. Status: accepted (fix/fresh-coach-input, final-QA finding Q5). Builds on
ADR 0085 (coaching-input-v2), ADR-R2-18 (the CoachingInput builder) and ADR 0064 (the chat).

## Context

The chat read `latest_coaching_input`: the input snapshot of the account's newest **report
run**. Reports are throttled (one manual report a day, the free plan's interval), so after every
profile rebuild the coach was up to a day behind. On the owner's data the only report was built
before the colour split and the peer benchmark (coaching-input-v1), so the coach said it had "no
separate White and Black results" and quoted 159 analysed games where the profile had 162.

The `ai` crate cannot call R2's builder: it depends on the `coaching-input` contract only, never
on `profile`.

## Decision

1. **A `coaching_inputs` snapshot table**, not `report_runs` with a kind. A report run has a
   lifecycle (status, retries, plan gate, a report row, batch items, the reconciler); a snapshot
   has none, so a run row would need every reader of `report_runs` to filter it out. A
   snapshot row is `(id, account_id, time_class, input_version, generated_at, input, created_at)`
   and is never updated.
2. **Written at the end of every `build-profile`**, by the profile crate, after the profile and
   repertoire rows are committed, with the same builder the report job uses
   (`profile::coaching::snapshot::store_coaching_snapshot`, the build's clock as `now`). No
   snapshot without analysed games. A failure is logged and never fails the build: the input is
   derived data, and the next build writes a newer one.
3. **The chat reads the most current stored input.** `latest_coaching_input` takes the newest
   snapshot and the newest report input of (account, time class) and uses the one with the newer
   `inputVersion`, then the later `generatedAt` (a tie goes to the snapshot). So a v1 report
   input never beats a v2 snapshot, and a report built after the last profile build is used
   as is. Accounts without a snapshot (before their first build on this version) keep reading
   the report input.
4. **Answers record their input.** `coach_messages` gains `coaching_input_id` (the snapshot,
   `ON DELETE SET NULL`) and `input_hash` (the sha256 of the input's canonical JSON, equal to
   `report_runs.input_hash` when the input was a report's). The validator and the evidence
   citations of a turn are built from that turn's input, as before; a stored answer is audited
   against its own snapshot, whatever is newer.
5. **Retention.** Each write prunes the account × time class to the newest 5 snapshots, except
   any a stored answer cites.
6. **Operator rebuild.** `cli report-rebuild (--account <uuid> | --platform <p> --username <u>)
   --time-class <tc>` stores a fresh snapshot, then queues a sync report run with
   `trigger = scheduled` (T18's `generate-report` job). The player's manual quota applies to
   `manual` runs only, and the beta and paid plans never gate a scheduled run; the free plan's
   interval still does. It is refused while a run of the account × time class is unfinished.

## Consequences

- The coach is as current as the profile, at the cost of one builder run per profile build
  (the same work one report's input costs) and a few snapshot rows per account.
- The web's report-derived views (weakness exemplars, report strengths) still read
  `report_runs.input`; they can move to the snapshot later.
