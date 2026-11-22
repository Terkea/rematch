# ADR-FIX-E-1. The coach finds the player's own moves by a move filter

Date: 2026-11-22. Status: accepted (bug hunt N8, owner's real data). Builds on ADR 0064 (the
coach chat), ADR-FIX-D-1 (units, cited-position boards) and bug hunt I10 (R2 occurrences).

## Context

- **N8.** Asked "Show me one rapid game where I blundered in a winning endgame", the coach said
  it found none and "the blunder search returned no examples". The owner's rapid games hold 45
  such moves in 24 games (`phase = endgame`, `judgement = blunder`, `win_before >= 70`).
- `find_examples` only took `weaknessId`, `motif` or `lineKey`, so a question about the player's
  own moves had no selector. The closest one, a weakness id, was described as "a weakness id from
  the coaching context", but with no band reference nothing is ranked (the owner's rapid input has
  no weaknesses), so the model was never told a real id (`W5`, `W8`, ...), although R2 stores
  exemplar plies for most of them (`game_metrics`: W1, W2, W3, W4, W5, W7, W9a, W9c, W10 and W8
  advanced; none for W6 or W8 basics).
- "Winning", "losing" and "equal" are engine words: the validator drops a paragraph that uses one
  without a `best`, `pv` or `eng` token. An honest "no endgame blunders from a winning position"
  has no position to cite, so it was removed too.

## Decision

1. **A move filter is a fourth `find_examples` selector**, exclusive with the other three, made of
   any of `phase`, `judgement` (exact) or `minJudgement` (that or worse, by the `judgement` enum
   order), `standing` (`winning` >= 70, `losing` <= 30, `equal` between, on the mover's
   `win_before`), `minWinBefore`/`maxWinBefore` (0..100), `minDeltaW` (1..100), `clockBelowSeconds`
   (1..3600), `color`, `since`/`until` (YYYY-MM-DD) and `lastGames` (1..500 newest analysed games),
   plus `sort` (`recent`, the default, or `worst`). `sort` alone is not a filter.
2. **Only the player's own moves** (`moves.color = games.player_color`) of the thread's account and
   time class match. Without `judgement`, `minJudgement` or `minDeltaW` only erring moves match.
   Each game gives one move, its biggest drop, newest game first (or biggest drop first). The
   records are the usual `PositionRecord`s, so the answer's boards render.
3. **The result says what it searched.** `searched` echoes the validated filter, the time class,
   the win-chance bounds in force (a preset's bound or a tighter given one) and the counts
   `gamesSearched`, `matchingMoves` and `matchingGames` (`CoachDataSource::count_examples`, a
   default method returning `None`), each with a `num` (or `date`) token in `refs`. The bounds
   print with their unit (`minWinBefore`, `maxWinBefore`, `minDeltaW`: `%`; `clockBelowSeconds`:
   ` s`), so "a win chance of at least 70%" replaces the engine word "winning".
4. **The weakness ids are in the schema.** `weaknessId`'s description lists every contract id with
   its plain meaning, so the model can use R2's stored exemplars when nothing is ranked.
5. **`chat/system` v2** teaches the move filter, prefers citing concrete moves (`move` and `best`
   tokens, so boards render), says that evaluation words need a `best` or `pv` token (else the
   win-chance number), and forbids a blanket "none found": an empty search names the time class,
   phase, judgement, win-chance bound and games searched, then offers a nearby search. v1 stays
   for stored hashes; `chat@1` stays the disclosure version, as `report@1` did for `core/tokens`
   v2 (ADR-FIX-D-1).

## Consequences

- The tool schema grows by 13 optional fields, all in the sidecar's JSON Schema subset.
- The filter runs two queries over the scope's games and moves (examples, counts); both are
  bounded by the account and time class and limited to ten examples.
