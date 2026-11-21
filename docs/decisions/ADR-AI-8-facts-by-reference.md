# ADR-AI-8: Facts by reference: tokenised narrative plus a strict leak check

Status: Accepted (placeholder number; renumbered at integration)

## Context

The claim validator originally tried to extract moves, numbers and dates from free model text and match them against the coaching input. Extraction is an arms race against notation tricks (zero-width characters, confusable dashes, figurines, full-width and Arabic-Indic digits, number words, glued SAN such as `Nf3Nc6`, long algebraic, `e8Q`), and every miss is a hallucinated fact shown to a user, possibly a minor.

## Decision

- Claude must not write chess moves, squares, numbers or dates as free text. Narrative uses reference tokens that the platform resolves against the CoachingInput and renders in the UI:
  - `{{move:<gameId>:<ply>}}`, `{{best:<gameId>:<ply>}}`, `{{pv:<gameId>:<ply>:best|refutation:<n>}}`;
  - `{{line:<lineKey>}}` and `{{line:<lineKey>:<n>}}`;
  - `{{num:<path>[|int|pct0|pct1|dec1|dec2|signed1]}}` and `{{date:<path>}}`, where paths come from the evidence path index (`listNumberPaths`; array items are keyed by id, key or index, and `.`, `|`, `:` or whitespace inside a key becomes `_`);
  - `{{sq:<square>}}`, validated only as a legal square name.
- `pct0` and `pct1` treat the value as a fraction (0.31 renders as 31%); fields already in percent points use `int`/`dec1`.
- `parseTokens` canonicalises text (invisible characters stripped, NFKC) and returns tokens, a stripped string with each token replaced by `§`, and syntax errors. Renderers must render the canonical form.
- `findLeaks(stripped)` is a deliberately aggressive detector: any digit in any script, any number word (including ordinals, fractions, dozen, "per cent"), square-like and SAN-like forms, long algebraic, castling with any dash, figurines, date-like forms, and the percent sign. False positives are acceptable because the model can repair by using a token; false negatives are not.
- Token resolution (a token that points at nothing, such as an unknown game id, ply or path) is a validation failure, as is any leak or syntax error.
- The older extractors (`extractMoves`, `extractNumbers`, `extractDates`) stay as defence in depth and for validating tool and chat output.

## Consequences

Prose such as "the fourth game" or "once more" is flagged and must be rephrased; the prompts must tell the model this. Evidence building must expose every fact the model may cite as a token path. Changing a path name is a prompt contract change.
