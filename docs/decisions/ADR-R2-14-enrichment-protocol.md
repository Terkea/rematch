# ADR-R2-14. Enrichment protocol

Date: 2026-11-22
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Rust port of TS M2's ADR 0030 (Task 22), done as R2 Task 25, with the owner's Maia-only override for difficulty.

## Context

The main analysis (`sf19-n300k-v1`: 300 000 nodes, MultiPV 1) gives one eval and one PV per position. The weakness detectors also need the second and third best moves (criticality, `in_top2`, the multipv gap), a refutation line for every mistake, what the opponent threatened (a null-move probe), exact endgame results (Syzygy) and how hard the move was for a player of that rating (Maia-2). Games evaluated from Lichess server evals have no PVs at all (`pv: []`), so motif tags and refutation lines would otherwise be empty and W1/W3 silently undercounted.

## Decision

### An enrichment layer with its own ids

- `enrich::enrich_game` runs after the main evals and never writes `position_evals`. Its results go to `move_features` (per ply) and two caches keyed by `fen_key` and their own protocol: `position_multipv` (`MULTIPV_PROTOCOL = "sf19-n300k-mpv3-v1"`) and `position_threats` (`THREAT_PROTOCOL = "sf19-n50k-null-v1"`). Every `move_features` row carries `ENRICHMENT_PROTOCOL = "enr-v1"`.
- **MultiPV never replaces the MultiPV-1 eval.** A MultiPV-3 search spreads its effort over three lines, so its first line can differ from the MultiPV-1 search in score and even in move. Accuracy, ΔW, judgements and `best_uci` stay those of the main protocol, so they stay comparable across games, players and references. The difficulty input's best move is the main eval's too; MultiPV lines are added beside it.
- **PV completion.** `best_pv = evals[i].pv` and `refutation_pv = evals[i+1].pv`. When the eval has no PV and the ply is critical, the MultiPV probe of `fen_before` (best) or of `fen_after` (refutation) supplies the first line's PV. Non-critical plies keep an empty PV: probing every ply would roughly double the cost.
- **Threat probe.** `THREAT_NODES = 50 000`, MultiPV 1, on `null_move_fen(fen_before)` for the requested colours' eligible plies (`decided = false`). It is skipped when the side to move is in check (the pass is illegal and Stockfish 19 exits on such FENs) and for the opponent's plies.
- **Tablebase.** Requested plies with at most `max_pieces` (5) pieces are probed; `tb_wdl_after = -probe(fen_after)`, because the probe answers for the side to move. A missing tables directory makes the probe unavailable (`None`), never a failure (ADR-R2-12).
- **Validation.** Every FEN is parsed with `Position::from_fen`, and positions without a legal move are skipped, before anything reaches the analyser.
- **Failure model.** An engine or store error fails the pass (`EnrichError::Engine/Store`), and cancellation returns `EnrichError::Cancelled` promptly. Maia and the tablebase never fail it.

### Critical-ply selection and its bias

`select_critical_plies`: a ply of a requested colour is critical when its ΔW ≥ 5, or the previous ply's ΔW ≥ 10 (the reply to a big swing). At most `MAX_CRITICAL_PLIES = 30` per game, keeping the highest ΔW (ties: the lower ply), returned ascending.

Known bias: a critical position that arrives without a prior swing and is played well is never probed, so `is_critical` and W9c undercount hard positions that were handled correctly. They are comparable only with references built with the same selection; reference builds must run this same pass.

### Difficulty: Maia only

Owner override (2026-11-21): difficulty comes only from Maia. There is no heuristic or engine-spread estimate anywhere.

- Requested eligible plies are batched to the `DifficultySource` (whose only implementation is the Maia-only `DifficultyEstimator`). Input lines are the MultiPV lines (mover win%), the main eval's best move with `win_before` when it is not among them, and the played move with `win_after`.
- Plies ≤ 10 are `not_applicable/opening_ply` and are never sent (Maia-2 was not trained on them).
- Maia missing, down, timing out or answering invalidly gives `difficulty = None` with `pending_maia/<reason>`. Enrichment completes, and `difficulty_model` is `None` unless some ply got a Maia estimate.
- `move_features` stores `difficulty_status` (`ok | pending_maia | not_applicable`), `difficulty_reason`, `difficulty_attempts` and `difficulty_call_failures` (migration `20261121020200`, a placeholder number for the coordinator).
- `best_pv` falls back to the MultiPV line that starts with the main eval's `best_uci`, or stays empty.
- **Re-enrichment** (`enrich::reenrich::reenrich_pending_difficulty`): a bounded, paged sweep (`page_size` × `max_pages`, keyset on `(game_id, ply)`, a partial index on pending rows).
  - It first checks Maia with a fresh `/healthz`, whose model id must match `^maia2-(rapid|blitz)-[0-9]+(\.[0-9]+)*$`. When Maia is not healthy the sweep changes nothing.
  - Inputs are rebuilt from the stored rows exactly as in the pass.
  - An `ok` answer sets `p_best`, `p_ok` and `difficulty_model`. A `not_applicable` answer is final.
  - Only an item-level invalid answer about a position that was sent and answered counts an attempt. At `MAX_DIFFICULTY_ATTEMPTS = 3` the row becomes `not_applicable/model_error`, so a deterministic model error is not retried forever.
  - A call-level failure counts no attempt, leaves the reason unchanged and stops the sweep. That covers transport errors, timeouts, and an unusable whole answer (`pending_maia/bad_response`: undecodable, wrong item count, or a non-Maia `/healthz` id). An outage or a broken Maia deploy never uses up the attempts.
  - When Maia is still healthy after such a failure, the rows it covered are charged one `difficulty_call_failures`. The sweep orders by that count, so a batch that always fails never blocks the rows after it. At `MAX_CALL_FAILURES = 5` a row is parked: it stays `pending_maia` and is never made final or estimated. `unpark_call_failures` makes parked rows eligible again, for example after a Maia redeploy.
  - Every sweep update re-checks that the row is still `pending_maia` with the counts it read, so a concurrent re-analysis or a second sweep is never overwritten or double-counted.
  - The sweep returns the changed games so the caller recomputes their metrics. When and how often to run it is the scheduler's call (Task 31/35).

### Cost

About +25–40% CPU over the main analysis: up to 30 MultiPV-3 searches at 300k nodes per game, plus one 50k-node threat probe per eligible player ply. Tablebase probes are cached in `position_tb`, and Maia runs in its own service.

### Re-running

Bumping `ENRICHMENT_PROTOCOL` re-enriches games from the caches without new main evals: the MultiPV and threat caches are keyed by their own protocols, which change only when their search settings change. `persist_move_features` replaces a game's rows as a whole (delete, then insert in chunks of 500 with `UNNEST`), so a re-run leaves one row per requested ply.

## Consequences

- Detectors read `move_features`. Any consumer of difficulty must handle `None` and never substitute a value.
- The TS M2 analyse-game hook (`move_features` in the same transaction as `moves`, `games.enrichment_protocol`) is wired by the Rust job tasks (31/35). `analysis` does not depend on `enrich` today.
- `persist_move_features` takes the per-ply difficulty outcomes as an extra argument beyond the TS signature, because the status must be stored for re-enrichment.
