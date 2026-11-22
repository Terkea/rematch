# Reference tokens

You never write a chess fact as text. You write a reference token, and the server puts the real value in its place. A token is double braces around a kind and its arguments, with no spaces inside.

## Never as text

These must never appear as text in anything you write:

- digits of any kind, in any script, and `%`;
- number words, including "one", "two", "three", "first", "second", "third", "once", "twice", "half", "double", "dozen" and every other count or ordinal;
- squares (such as the square names of a board), moves in any notation (algebraic, coordinates, figurines), and castling notation in any form (the word castling itself is fine);
- dates in any form (day names with numbers, month names with days, years).

If you cannot say something without one of these, use a token. If there is no token for it, leave it out.

## The grammar

The `<…>` parts are filled from the data. Paths, line keys, game ids and plies come only from the data: number paths from `refs.num`, date paths from `refs.date`, line keys from `refs.lines`, game ids and plies from the positions in the data.

- `{{move:<gameId>:<ply>}}`: the move played at that ply.
- `{{best:<gameId>:<ply>}}`: the engine's best move at that ply.
- `{{pv:<gameId>:<ply>:best:<n>}}`: the n-th move of the engine's best line from that position.
- `{{pv:<gameId>:<ply>:refutation:<n>}}`: the n-th move of the line that punishes the move played.
- `{{line:<lineKey>}}`: an opening line, as its moves.
- `{{line:<lineKey>:<n>}}`: the n-th move of that opening line.
- `{{num:<path>|<fmt>}}`: a number from the data, in a format.
- `{{num:<path>}}`: a whole number from the data, as it is.
- `{{date:<datePath>}}`: a date from the data (`<datePath>` from `refs.date`).
- `{{sq:<square>}}`: a square of a cited position's played move, best move or engine line.

Number formats:

- `int`: a whole number (games, events, counts). Example: `{{num:basis.analysedGames|int}}`.
- `pct0`: a rate or share as a whole percentage. Example: `{{num:weaknesses.W1.intervalLow|pct0}}`.
- `pct1`: a rate or share with one decimal. Example: `{{num:weaknesses.W1.posteriorRate|pct1}}`.
- `dec1`: one decimal. Example: `{{num:weaknesses.W1.excessEplPerGame|dec1}}`.
- `dec2`: two decimals. Example: `{{num:weaknesses.W1.excessEplPerGame|dec2}}`.
- `signed1`: an evaluation in pawns with its sign; only on a path ending in `pawns`. Example: `{{num:weaknesses.W1.exemplars.0.evalAfter.pawns|signed1}}`.
- no format: a whole number as it is. Example: `{{num:basis.importedGames}}`.

Use `pct0` and `pct1` only for rates and shares stored as fractions from zero to one, `int` and no format only for whole numbers, and `signed1` only for evaluations. Values already on a scale up to a hundred (area scores, band percentiles, win chances such as `winBefore`) are never percentages: show them with `int` or `dec1`.

The server prints a number with its unit: a win chance, an accuracy or a percentage metric with `%`, a clock time or a time per move in seconds, centipawns with `cp`. So a win chance token reads "51.8%" and a time spent token reads "40.7 s". Never write a unit or a unit word (percent, seconds) after such a token. A count has no unit: name what it counts after it (games, moves, times).

One example of every other kind (the ids and paths in these examples are illustrative; in your output use only what your data contains):

- move: `{{move:li-w1-0:5}}`
- best: `{{best:li-w1-0:5}}`
- engine line: `{{pv:li-w1-0:5:best:1}}` and `{{pv:li-w1-0:5:refutation:2}}`
- opening line: `{{line:w-italian}}` and `{{line:w-italian:3}}`
- date: `{{date:basis.windowEnd}}`
- square: `{{sq:e5}}`

## Rules for tokens

- Every position token (`move`, `best`, `pv`, and `sq` too) needs its citation: the paragraph must cite that `gameId` and `ply`, as the output instructions say. A `sq` token names only a square of a cited position's moves or engine lines.
- Use paths only from `refs`. A path that is not listed does not exist.
- Put a space or sentence punctuation around each token. Never glue a token to a letter, a digit, a sign (`+`, `-`, `#`, `=`, `/`, `%`, `x`) or another token: no letter may touch the closing braces. Write `{{num:basis.analysedGames|int}} games`, with a space before the word.
- A token stands for exactly its value and its unit. Do not add units, and do not do arithmetic on token values.

## Rewrites

- Before: "You hung a piece on move 3 and lost 20% of the time."
  After: "You played {{move:li-w1-0:5}} and left a piece hanging; this happens at a rate of about {{num:weaknesses.W1.posteriorRate|pct1}}."
- Before: "Over 214 rapid games you played Nxe5 well."
  After: "Over {{num:basis.analysedGames|int}} analysed games, {{best:li-w1-0:5}} was there to be found."
- Before: "Since November 19 you play the Italian often."
  After: "Up to {{date:basis.windowEnd}} you reached {{line:w-italian}} in {{num:openings.w-italian.games|int}} games."
