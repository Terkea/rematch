# 0049. Game review data: PGN-first plies, stored engine scores, honest gaps

Date: 2026-11-22. Status: accepted.

## Context

The single-game review page (M5a Task 10) replays one game move by move with an eval bar, an eval graph, judgements, best-move arrows and clocks. The TS worker has already stored everything it knows: the PGN in `games`, one `moves` row per ply (mover-POV win% before and after, judgement, best move, phase, time spent), and `position_evals` (White-POV engine scores keyed by `fenKey` and protocol). The loader must read this without inventing anything, show a game whose analysis is not done, and never confirm that another user's game id exists.

## Decision

- **Loader.** `getGameReview(db, userId, gameId)` in `apps/web/src/server/games/review.ts` returns a `ReviewGame` or null. It checks `gameId` against a uuid pattern before any query, then runs in `readOnly` and selects the game `where id = $1 and account_id in scopedAccountIds(user)` (ADR 0046). A malformed id, a random uuid, another user's game and a game of an account nobody links all give null, which the page turns into `notFound()`.
- **PGN-first plies.** `plies` always come from `parsePgn(game.pgn)`, so every game can be replayed, whatever its analysis status. A PGN that `parsePgn` rejects (a variant, a custom start) gives no plies rather than an error.
- **Mismatch rule.** `analysis` is attached only when `analysis_status = 'done'` and the `moves` rows line up with the PGN: the same count, and row *i* has ply *i*, the same colour and the same `uci`. Otherwise every `analysis` is null, `analysed` is false, `moments` is empty, `startScore` is null and `phases` is `{}`. A partial or stale analysis is shown as no analysis, never as a half-filled graph. Accuracies are likewise shown only for `done` games, because a re-queued game keeps its old values.
- **R7, engine scores.** `score` (White's point of view) is the `position_evals` row for `fenKey(fenAfter)` under the game's `analysis_protocol`, fetched for the start and every position in one `fen_key = any($keys)` query. When the row is missing, the score is derived from the stored win%: `scoreFromWin(winWhite)` inverts `winningChances` exactly, rounds to an integer and clamps to ±1000 (the cap `winningChances` applies), and the score is marked `scoreSource: "derived"`, which the page shows with "≈". The final position of a checkmate scores `{ mate: 0 }` (source `engine`; it is a fact, not an estimate). `startScore` follows the same rule, deriving from the first move's `winBefore`.
- **White's point of view.** `moves.win_after` is the mover's win%; `winWhite` is it for White's plies and `100 − win_after` for Black's, matching `buildMoveRecords` in `@ci/chess-core`.
- **R7, classification.** Only what is stored: `blunder`, `mistake` and `inaccuracy` from `moves.judgement`, and `best` when the played `uci` equals the stored `bestUci` and the move carries no judgement. There is no `!` or `!!`, because nothing stored supports brilliance.
- **Moments.** The player's own judged plies (`moves.color = games.player_color`), worst first by `deltaW`, ties broken by the lower ply. They drive the review's opening ply and the reveal's climax (ADR 0052).
- **Clocks.** `clockAfter` is the mover's `[%clk]` after the move, from the PGN; `hasClocks` is true when any ply has one.
- **Source-URL allow-list.** `sourceUrl(platform, headers)` reads `Link` for chess.com and `Site` for Lichess, and accepts only a plain `https:` URL on `www.chess.com` or `lichess.org` respectively, with no port and no credentials. Anything else (`javascript:`, look-alike hosts, `http:`) gives null, so a crafted PGN header can never become a link.

## Extension points

- `ReviewPly.analysis` gains R2's motif tags (fork, pin, hanging piece, …) as an extra field; consumers that ignore it keep working.
- `moments` (game id plus ply) are the anchors for R3's "Ask your coach" entry points.

## Consequences

The review never shows a number the worker did not store or that is not an exact inverse of one it did, and a derived score is always marked. A game whose analysis is re-run shows as pending until the new rows land, rather than mixing old and new analyses. The one extra query per review (the eval lookup) is bounded by the game's length.
