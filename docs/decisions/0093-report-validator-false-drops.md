# 0093. The report validator drops claims, not honest wording

Date: 2026-11-23. Status: accepted (R2-1, holistic review 2). Narrows the language rules of ADR
0060 and the leak detector of ADR-AI-8 at three points; the prompt versioning rule of Task 13
holds (a new prompt file, the old one kept for stored hashes).

## Context

The owner's rapid report v5 said "8 statements were removed because they couldn't be checked
against your games". `report_sections.dropped` showed three causes, none of them an uncheckable
claim:

- (a) `unresolved_token` `{{num:areaScores.<area>.needMore}}` for middlegame, calculation, time
  and mindset. These areas are insufficient with `needMore` null: their components have no
  band-reference stat (more games cannot fix that), and Mindset is never scored
  (`profile-core` `compute_area_scores`). The validator still required the `needMore` token for
  every insufficient area, and the prompt asked for it, so the item was dropped either way:
  without the token (`insufficient_data_not_stated`) or with it (`unresolved_token`).
- (b) `free_text_fact` `number_word:"first"`: "First, …", "your first move".
- (c) `uncited_reference` "engine claim 'checkmate'/'mates'" in climb-plan drill rationales and
  the weekly rhythm ("checkmate puzzles"), texts the schema forbids tokens in, so they could
  never carry engine evidence.

## Decision

1. **Areas.** `states_insufficient_data` applies only to an insufficient area whose `needMore`
   is a number. `report/section-areas` v2 tells the model that a null `needMore` means no score
   yet, with no count and no token, as the web's "No score yet" and Mindset's "coaching advice,
   not a score" say. The coaching input is unchanged: it already carries `needMore` (in games)
   wherever more games would give a score. Inventing a count for the other areas would be false.
2. **Discourse ordinals.** The claim validator (`validate_text`, stage 3) drops a `number_word`
   leak that is "first" anywhere, or "second"/"third" when every use opens a sentence and is
   followed by a comma (`leaks::is_discourse_ordinal`). `find_leaks` itself is unchanged (TS
   parity). A counted claim keeps its own leak ("first 10 games", "first ten games"), and "a
   third of your games" or "your second loss" is still caught.
3. **Mate words in training advice.** `engine_claims` skips a mate word (`mate`, `mates`,
   `mated`, `checkmate…`) when the paragraph names no position (no citation, no `move`, `best`,
   `pv`, `eng` or `sq` token), a training word (puzzle, pattern, drill, practise, look for, …)
   is within three words, and its sentence has no claim cue ("you had/missed/allowed", "there
   was", "this move", "in your game", "engine"). Every other engine word, and any mate word in a
   claim, still needs engine evidence.

The core rules (`core/rules` v1, `core/tokens` v2) still ask the model to avoid ordinals and
engine words. They are shared with the chat and weekly prompts, so they stay as they are: the
prompt is stricter than the validator, which is safe.

## Consequences

- An honest report keeps its area notes, plan intro, drill rationales and weekly rhythm.
- The report prompt set hash changes (areas v2), so new reports record the new set.
- False negatives the rules accept: "first" as a rank ("you finished first") and a mate word in
  advice that implies a game claim without a cue. Neither is a number or position fact a token
  could cite.
