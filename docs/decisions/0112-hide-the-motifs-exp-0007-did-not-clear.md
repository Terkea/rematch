# 0112. Hide the motif tags EXP-0007 did not clear

Date: 2026-11-24. Status: accepted (lane `hidetags`, branch `feat/hide-untrusted-motifs`, issue #122). Builds on ADRs 0104, 0105, 0109 (Amendment 1) and ADR-R2-3. Ships as `algo-3.0.0`.

## Context

EXP-0007 scored the production motif tagger against Lichess's generated puzzle themes on 54,987 puzzle solutions (`lab/experiments/EXP-0007/decision.md`). By its registered rule (precision with a Bonferroni interval wholly above 0.80) it trusted `fork` (0.976), `discoveredAttack` (0.992), `promotion` (0.998), `smotheredMate` (1.000) and the five mate-length tags, and hid six: `skewer` (0.810, interval [0.778, 0.838]), `backRankMate` (0.606), `hangingPiece` (0.572), `deflection` (0.562), `sacrifice` (0.528) and `pin` (0.402, recall 0.201). `matingNet` has no Lichess theme and was not scored. Until #122 the product still named all sixteen: a `motif.missed:<tag>` and `motif.allowed:<tag>` series per tag, a `motif_miss_rate:<tag>` KPI row on the Tactics page, a timeline change ("Missed pins: emerging"), the tags of every exemplar in the CoachingInput, the chat's `find_examples` and its positions' tags, the coach's glossary, and the trainer's "Is it working?" pattern rates.

## Decision

1. **One allowlist, `coaching_input::motifs::PLAYER_VISIBLE_MOTIFS`.** Ten camelCase tags: EXP-0007's nine trusted ones and the unscored `matingNet`, which keeps the status it had. `PLAYER_VISIBLE_MOTIFS_SOURCE` is `"EXP-0007"`. It lives in `coaching-input` because `profile-core`, `profile` and `ai` all depend on that crate and no other. `backend/crates/coaching-input/tests/motifs.rs` reads EXP-0007's `results.json` and fails unless the list is exactly `decisions.trusted` plus `unscored`, and none of the six.
2. **It is an algorithm component.** The manifest names it (`profile.player_visible_motifs`, with its source), so `scripts/algorithm/check.mjs` requires a major bump whenever it changes. Clearing a tag after EXP-0019, EXP-0020 or EXP-0021 is one line in the list, the test's source moving to that experiment, and a new major version.
3. **Computed, never exposed.** The tagger (`motifs/tag_line.rs`), the stored `move_features` lines and `cli lab tag-motifs` keep all sixteen tags, so the arena and the follow-up experiments keep measuring them. Everything a player or the coach reads is cut at the profile:
   - `motif_missed` and `motif_allowed` (`profile-core/src/weakness/detectors.rs`) build series for visible tags only, so a hidden tag has no `weakness_series_point`, no `missedByMotif` count, no timeline row and no `weakness.changed` event. The move still counts for W3 or W1.
   - The KPI catalogue has a `motif_miss_rate` row per visible tag: 63 rows, not 69.
   - Rows stored before `algo-3.0.0` are skipped where they are read: `load_series_points`, the CoachingInput's timeline changes, the web's area timeline (`isPlayerVisibleSeries`), a stored report's new and fixed changes and their notes (`report/load.ts`), an exemplar's tags on the weakness pages, the trainer's pattern rates in Rust (`pattern_found`) and the web (`store.ts`), and a stored CoachingInput the chat reads (`without_hidden_motifs`: its hidden metrics, changes and tags).
   - An exemplar's `tags` are its visible tags; a position whose only tags were hidden has none and still carries its move, its evals, its best line and its refutation.
   - The chat drops hidden tags from a position's stored line tags, and `find_examples` with a hidden motif finds nothing. Its tool description no longer offers `pin` or `hanging_piece`.
   - The coach's glossary is regenerated as `core/glossary.v3.md` without the six; the report and chat sets use it. v2 stays for stored hashes.
   - The web reads the list from `algorithm/manifest.json`, filters its KPI catalogue by it, and names a hidden motif series by the fallback Rust already had, "Missed tactics" (`series_label`) and "Allowed tactics".
4. **Nothing else is hidden.** W1 ("Leaving pieces hanging") is a weakness rule on the refutation's material, not the `hangingPiece` tag, and stays. A blunder stays a blunder with its eval swing and the best move.

## Options considered

1. **Stop computing the six tags.** Rejected: EXP-0019 to EXP-0021 measure them, and the arena compares every version on EXP-0007.
2. **Hide them only in the web.** Rejected: the coach reads the CoachingInput, the glossary and the chat's positions, and a timeline event would still reach the weakness-change prompt.
3. **The list in `profile-core`, typed as `MotifTag`.** Rejected: `ai` does not depend on `profile-core`, and a second copy for the chat would be two lists to keep equal. `MotifTag::is_player_visible` reads the one list.
4. **Hide `matingNet` too.** Not adopted: EXP-0007 found no evidence against it, and #122 names the six. It has no ground truth either way; an experiment that scores it is the way to settle it.

## Consequences

- The release arena against `algo-2.0.0` finds no clear difference on any of EXP-0001's 3, EXP-0002's 2 and EXP-0007's 31 metrics: the hidden tags are measured exactly as before, and nothing ranked or calibrated was built on them. The replay goldens lose every row of the six tags (112 of 435 series points, 234 of 2,964 KPI snapshots, 45 of 195 timeline rows).
- Players who had a "Missed pins" row or change see it go on their next page load, stored reports included; the moves behind it stay in "Missing a short tactic" and "Leaving pieces hanging". A change note queued before the upgrade may still be written, but its change is never shown, so neither is the note.
- Every rebuild after the upgrade deletes the stored timeline rows of the hidden series (the history is replaced for the current detector version); until then the readers skip them.
