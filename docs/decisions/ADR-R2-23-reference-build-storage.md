# ADR-R2-23. Reference build storage: lifecycle guards and per-metric band fallback

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Task 17 (`backend/crates/refdata`), with the Task 17 review ruling. Related: ADR-R2-10 (explorer build), ADR-R2-11 (band reference method), ADR-R2-13 (Maia-only difficulty).

## Context

Reference data (the opening explorer and the band references) comes from quarterly builds over the CC0 Lichess dump. Profiles compare a player with players at their rating, so a wrong or half-built reference set misleads every user. A band × time-class cell can also be thin: some metrics have fewer than 10 players (`low_sample`) while others in the same cell are fine.

## Decision

### Build lifecycle

- Status flows `building` → `active` → `superseded`. The dead ends are `failed` (rows deleted) and `unverified` (`--verify none` development builds; rows kept, never active).
- `activate_build` (and `activate_build_in`, which runs inside the caller's transaction) holds a per-kind advisory lock. It supersedes the old active build, deletes that build's rows, and only then activates the new one, as two separate statements. They must be separate because `reference_builds_one_active_uq` is a plain unique index and cannot be deferred.
- Activation is refused in each of these cases:
  - no SHA-256 recorded (R-9);
  - any status other than `building` (an `active` build is a no-op);
  - a `source_month` that is not newer than the active build's, so an old dump never replaces a newer one. Months are validated as `YYYY-MM` at creation, so string order is month order.
- **Every write is guarded.** Each of these requires the build to be `building`, and gets `NotBuilding` otherwise: explorer flush and prune; reference-game insert and status update; observations; band-reference rows; the SHA (exactly 64 lowercase hex characters); and the difficulty model. A served build is therefore immutable, and a failed build cannot gain rows back.
- Observations must belong to their game's build (`BuildMismatch`), and an upsert never moves a row between builds.
- `delete_stale_unverified` removes `unverified` builds 30 days after they finished (`UNVERIFIED_RETENTION`), with their rows via the FK cascade.
- `difficulty_model` is stored exactly as given. `None` stays NULL (ADR-R2-13 owner override).

### Per-metric band fallback (review ruling, option a)

`load_band_reference(band, tc)` serves **each metric independently**, from the nearest band in the same time class whose row for that metric is not `low_sample`:

- The search goes at most **2 bands** away (`MAX_FALLBACK_BANDS`). Beyond that, the metric is absent.
- Of two equally near bands, the lower one wins, because unknown players are more likely beginners (ruling Q2).
- A `low_sample` row is never served, not even to label it.
- The result records the band each metric was served from (`ServedStat.band`, `served_from`, `is_fallback`), so reports can say "compared with B2 players" honestly.
- Rows whose metric this version does not know are ignored, so a cell holding only unknown metrics falls back like an empty one.
- `None` means there is no active build, or no metric within reach.

## Alternatives rejected

- **Whole-cell fallback** (the first implementation). It treated a cell as low-sample only when *every* row was, and then served that cell's low-sample rows as reliable. In the review's case (B3 with a low-sample W1 next to healthy metrics), it served B3's thin W1 as a benchmark.
- **Unlimited fallback distance.** It would compare a B5 player with B0 players, which is not a meaningful benchmark.

## Consequences

- **T32** must either commit its band-reference and observation writes before `activate_build`, or write and activate in one transaction with `activate_build_in`. It treats `NotBuilding` from a late worker as "already done". It must also handle jobs whose `reference_games` row was deleted by a later supersede.
- **T20:** `flush_explorer` adds counts, so it is not retry-safe. The caller makes each chunk idempotent, or fails the build and starts over.
- **Consumers (T12, T15, T33)** read `LoadedBandReference::reference()` for the statistics and `served_from` for labelling. A metric missing from the map means there is no benchmark.
