# 0068. Coach read models: scope, exemplars, sufficiency and the catalogue

Date: 2026-11-22. Status: accepted (M5b Task 1). Builds on ADR 0046 (the games read model,
`readOnly`, `scopedAccountIds`), ADR 0059 (R3 tables and the web hand-off), ADR 0064 §9 and
§19 (the chat's input snapshot and account scope) and ADR-R2-7 / ADR-R2-13 (KPIs, Maia-only
difficulty).

## Context

M5b's coach pages (report, weaknesses, areas, progress, chat) read the R2 profile tables:
`weakness_estimates`, `weakness_state_history`, `weakness_series_point`, `kpi_snapshot`,
`game_metrics`, `game_openings`, `move_features`, and `report_runs.input`. Each of them is keyed
by (account, time class), labels and minimum evidence live in Rust, and the web must not invent
numbers the backend did not compute. `apps/web/src/server/profile` is that read model.

## Decision

1. **R1, the coach scope is one (account, time class).** The options are the user's linked
   accounts (`scopedAccountIds`, verified or not) with at least one `game_metrics` row, best
   first: most rows, then lichess before chesscom, then bullet, blitz, rapid, classical, daily.
   The URL params `account` and `tc` narrow the options when they name one of them; a foreign,
   malformed or empty param is ignored, and a choice that matches no option falls back to the
   first option. With no option, `resolveCoachScope` returns null (the "building" state).
2. **Every read is scoped and read-only.** Each exported read runs in `readOnly` and first
   checks that the scope's account is one of the user's (`ownsScope`), so a scope built for one
   user reads nothing for another: empty lists, null details and zero counts of rows, never
   another user's data. Every query filters by the scope's account and time class.
3. **R2, exemplars come from the newest report input.** A weakness's example positions are the
   `exemplars` of the matching `weaknesses[]` entry (contract `id` and `variant`, e.g. `W3` +
   `shallow`) of the newest `report_runs` row of the scope whose `input` is not null, the snapshot
   the coach chat also reads. The page names that run's date. No such run gives `null`; a run
   that cites no position for the weakness gives an empty list. Only `input` and `created_at` are
   read from `report_runs`.
4. **R3, the headline number is the stored posterior.** A weakness shows `mean` with its stored
   `ci_low`–`ci_high` (80%), next to `k_unweighted` of `n_unweighted` chances. It is sufficient
   when `n_unweighted ≥ min_evidence(key)` (Rust); below that `needMore` is the shortfall. A
   series without a Rust minimum (only binomial weakness keys have one) is never sufficient. The
   lead finding is the top weakness with `priority > 0`, state `active`, `emerging` or
   `relapsed`, and sufficient evidence.
5. **Intervals are stored or Wilson.** Stored intervals are passed through unchanged. The only
   interval the web computes is the weekly weakness rate: `wilson(events, chances, 0.8)` over a
   week's `weakness_series_point` rows (ISO weeks in UTC, the newest 26 with a point), and a week
   with no chance has `rate: null`. KPI and area values that are null stay null; a missing area
   score is `{ value: null, n: 0 }`.
6. **Catalogue parity.** `catalogue.ts` holds the weakness, motif and KPI tables as TS literals.
   `profile-core`'s `catalogue_snapshot` test writes them from the public Rust items to
   `backend/fixtures/profile/catalogue.json` (`UPDATE_SNAPSHOTS=1`) and fails on drift; the web's
   `catalogue.test.ts` fails when the TS tables differ from that file. The seven areas are kept in
   `@ci/ui`'s `areaLabels` order by a test, because `area-score` is a client module.
7. **Labels.** `W*` keys use the Rust label. `motif.missed:<tag>` uses the KPI label of
   `motif_miss_rate:<tag>` ("Missed forks") and `motif.allowed:<tag>` the same words with
   "Allowed" ("Allowed forks"). `opening:<colour>:<slug>` uses the line's `game_openings.line_name`,
   else the slug in words, plus "(as White)" or "(as Black)". `endgame:<type>` reads "Rook
   endgames", "Minor-piece endgames" and so on. An unknown key is shown as itself.
8. **Areas of non-`W` series.** `motif.*` series belong to tactics, `opening:*` to openings and
   `endgame:*` to endgame; an unknown `W` variant takes its family's area.
9. **Difficulty is counted, never estimated.** `difficultyStatus` counts the scope's positions by
   `move_features.difficulty_status` (`ok`, `pending_maia`, `not_applicable`); positions without
   a status are not counted.

## Consequences

- A new Rust weakness, motif or KPI fails the Rust snapshot until the fixture is regenerated,
  then the web parity test until `catalogue.ts` is updated, so labels and thresholds never drift.
- Pages get honest gaps (null, `needMore`, `rate: null`) and must render them as stated gaps.
- Exemplars lag the profile until the next report is built; the page says which report they
  come from.
