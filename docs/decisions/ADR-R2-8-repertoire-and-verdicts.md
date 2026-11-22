# ADR-R2-8. Repertoire tree, line keys, struggles and opening verdicts

Date: 2026-11-22
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Task 18 (`profile-core::openings::{repertoire,lines,verdicts}`, `refdata::repertoire`). Ports TS M2 Task 13 and its ADR 0041. Related: ADR-R2-23 (reference storage and band fallback), ADR-R2-5 (interval conventions).

## Context

The coach tells a player which of their openings work and which do not ("your Caro-Kann scores 31%"). That claim is only useful if it is honest: keyed by what the player actually reached, attributed to the side that actually chose the line, and backed by enough games. Line keys are persisted in `game_openings`, `opening_lines` and series keys (`opening:<line_key>:<side>`), so their format must be fixed before any row is written.

## Decision

### Tree

- One tree per account × time class × colour, a **DAG keyed by `fen_key`** (placement, side, castling, en passant only when a capture is legal; no clocks). Transpositions merge into one node with several parents.
- Only the first **30 plies** of a game enter it. A game counts once per node and once per edge, even when it repeats a position (`Nf3 Ng1 …`).
- Node stats: games, W/D/L, score, `perf_vs_expected` (mean of `score − expected` over the games with an expected score; `None` without any), analysed games and the means of win% at the opening end, opening accuracy and time in the opening, each over the games where it is known. Children by games desc, then uci; parents sorted.

### Lines

- A game's line is the **`line_family` (text before the first `,`) of the deepest named position** among its first 30 plies, from the CC0 chess-openings dataset (Task 16). "Deepest" is the last named ply in game order, so lost tempi can select a shallower name: `1.e4 e5 2.Nf3 Nc6 3.Ng1 Nb8` is labelled C20, not C44. Plies are numbered from 1 in the game's own order (for a custom start, ply 1 is the first move after it); custom starts and variants do not parse and give no line.
- **Line keys (R-7, Task 16, fixed from here on):** `"{color}:{slug}"`, where `color` is the player's colour. The slug is lowercase ASCII: diacritics ASCII-folded (ü→u, ø→o …), apostrophes dropped, every other run of non-alphanumerics one `-`, `unnamed` if empty. A slug over 40 characters is cut at a `-` boundary to ≤ 33 and gets `-` plus 6 hex digits of `sha256(full slug)`, so the slug is ≤ 40, a key ≤ 46 and a series key ≤ 54 (contract 64). Changing any of this needs a data migration of `game_openings`, `opening_lines` and the opening series.
- **Ownership (T16 controller ruling).** `entry_mover` is the factual mover of the first ply that carries the family name (`Classified.entry_ply` / `entry_mover`, never the deepest ply's mover), and it follows each game's own move order. Because many Black defences are first named on a White move (Old Indian after `d4 d6 c4`), the side is decided by an explicit rule on the family name, whole words:
  1. contains "Reversed" → White owns it (checked first: "Zukertort Opening: Reversed Mexican Defense" is White's);
  2. contains "Defense", "Defence", "Declined", "Accepted" or "Countergambit" → Black owns it;
  3. otherwise the `entry_mover`.
  The player **chose** the line when they own it, else they **faced** it. Lines are grouped by `(line_key, kind)`; a family both chosen and faced gives two rows and is never merged (`opening_lines` PK includes `kind`).
- `line_entry_ply` is the family's first named ply. The **choice node** of a chosen line is the position before the player's last move at or before the entry ply (the entry ply itself when the player made it).

### Theory and book exit

- **Theory** is our own rating-band explorer (Task 17): a move is theory when its position has ≥ 50 explorer games and the move has ≥ 5% of them (exact integers). `profile-core` mirrors `refdata::explorer::in_theory` because it cannot depend on `refdata`; a refdata test pins that both agree.
- `book_exit(plies, index, player)` returns `(book_exit_ply, player_deviation_ply)`. The book exit is the first ply that is not theory. It is the **player's deviation** only if the player made that move in a theory position (≥ 50 games). When the position is thin, theory ran out and nobody deviated; when the opponent moved, the player did not deviate.
- **No claim from zero evidence:** if the first position is not a theory position (for example no explorer data for the band), both are `None`.
- The line's **median deviation ply** treats games where the player never left theory as later than any ply, and is reported only with ≥ 8 games of theory evidence and a finite median. `deviation_ply` is then the lower median (an observed ply) and `deviation_move_san` the most common move played there.

### Band of the comparison

- The player's band follows R-4 with ruling Q2: the latest rating in the time class (`to_lichess_equivalent`), else the mean opponent rating of the last 20 games, else **B1**. Its source (`rating`, `opponent_mean`, `default`) is recorded.
- The explorer index is loaded for that band. If the band has no explorer data for any of the player's positions, the nearest band with data is used, at most **2 bands** away (lower band first on a tie), as for the band reference (ADR-R2-23). Beyond that there is no comparison.
- Every line summary carries `comparison { playerBand, bandSource, explorerBand, explorerFallback }`, and every reason that cites the band (theory, alternatives) names the band used and says "fallback band" when it is not the player's own. Task 34 must carry this into `Alternative.bandScore` wording.

### Struggles (TS `STRUGGLE_THRESHOLDS`)

`{ minGames 8, perfDrop −0.10, worseWin 40, minAnalysed 5, deviationMinGames 3, deviationShare 0.3, deviationDeltaW 10 }`. A line with fewer than 8 games has no struggles.

- `score_below_expected`: perf ≤ −0.10 over ≥ 8 games with an expected score, and the upper end of the 80% normal interval (`Z80`) < 0. The interval's variance is the larger of the sample variance of the residuals and the mean null variance `E(1 − E)`, so equal residuals (15 losses at expected 0.2) still give a real interval; a degenerate interval is `None` and claims nothing.
- `worse_after_opening`: ≥ 5 analysed games with a win% at the opening end, mean ≤ 40.
- `repeated_deviation_error`: one `(fen_key, uci)` **of the player** with ΔW ≥ 10 in ≥ 3 games and ≥ 30% of the line's games. The opponent's errors are not the player's fix. That spot is the line's `fix_spot`.
- Engine data counts only from a `game_metrics` row of the current `PLAYER_GAME_METRICS_VERSION` (plus its `moves` rows); a stale row is no evidence, never zero evidence. Results and ratings come from `games`, so every imported game counts for score and performance.

Means and shares are compared with a tolerance of 1e-9, because float sums of exact inputs are off by a few ulps.

### Verdicts (rules in order)

1. games < 8 → no verdict (`None`; omitted from the contract);
2. `repeated_deviation_error` → `fix`;
3. `score_below_expected` ∧ games ≥ 15 ∧ perf ≤ −0.15 ∧ chosen → `replace`. **A faced line is never replaced** (the player did not choose it). Alternatives: the top 3 explorer moves at the choice node with ≥ 100 games, excluding every move the line's games played there, by score for the side to move;
4. chosen ∧ median player deviation ply ≤ 10 ∧ the line has ≥ 10% of the player's games with that colour → `learn`;
5. any other struggle → `learn`;
6. otherwise `keep`.

**Playstyle-clash replacement is deferred** until playstyle axes exist (spec §8a).

### Eval at the opening end (R-8)

`avg_eval_white_cp` is the mean White-POV `score_to_cp` (mate = ±1000) after the last opening ply over the line's analysed games, rounded to an integer, with its game count; `None` without analysed games.

### Storage

`rebuild_repertoire(pool, account, tc)`:
- fills missing `game_openings` rows for games of **every status** (`imported` included) from their PGN; an unparsable PGN gets a row with empty arrays and no line, so it is not parsed again and adds no evidence;
- refreshes `book_exit_ply` and `player_deviation_ply` for every game from the current explorer, because a new explorer build changes them;
- replaces `repertoire_nodes` (stats = the node as camelCase JSON) and `opening_lines` (summary = the `LineSummary`) for the account × time class in one transaction, under a per-account × time-class advisory lock.

## Consequences

- Classification is stored once per game. A change to the dataset, the slug rules or the ownership rule needs `game_openings` rows of the affected games to be deleted and rebuilt.
- The two `in_theory` copies must change together (pinned by a test). A later cleanup could move `ExplorerMove` into `profile-core` and re-export it from `refdata`.
- Struggle cost ranking ("games lost × frequency", spec §8a) is not part of this task.

## Alternatives rejected

- **Ownership by entry mover alone.** About 1 000 "Defense" rows are first named on a White ply, so Black's Old Indian would read as faced.
- **Book exit at ply 1 without explorer data.** It would claim that every player leaves theory immediately when the band has no explorer build.
- **Median deviation over the deviating games only.** Two early deviations among 20 games that stayed in book would read as "leaves theory at ply 6".
- **Plain sample-variance interval.** Equal residuals give a zero-width interval, which claims certainty the data does not have.
