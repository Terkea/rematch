# ADR-R3-2. Porting the TS AI reference to Rust, and how parity is kept

Date: 2026-11-21
Status: Accepted (R3 Task 5). Placeholder number; Task 26 renumbers it. Builds on 0030, ADR-R3-1 and ADR-AI-8.

## Context

R3 ports TS M3 (`.worktrees/m3/packages/ai`) to the Rust `ai` crate. The TS code and its tests are the behavioural spec. Most of it ports one to one. A few things cannot, mostly because the `regex` crate has no lookarounds. In exchange it guarantees linear-time matching, with no catastrophic backtracking. The validator enforces facts by reference (ADR-AI-8). A port that drifts from TS, or that a hostile string can stall, would leak made-up chess facts or hang a job.

## Decision

### Intentional deviations

These are the plan's eight deviations. Every other behaviour must match TS.

1. `parse_coaching_input` reports the first serde deserialisation error, then every refinement error. Zod reported all issues at once. Paths stay dotted (`weaknesses.0.rank`).
2. Regex lookarounds are emulated by `validator::bounded` (`bounded_matches` and its general `scan`). A failed boundary check retries from the rejected start plus one char. JS `\d` becomes `[0-9]`, and JS `\b` becomes `(?-u:\b)`. `Token.index` is a byte offset into the canonical text, not a UTF-16 index.
3. The number and date path index is a `BTreeMap`, so `list_number_paths` returns paths sorted, not in insertion order.
4. The model never writes free-text facts. The old extractors (`extract_moves/numbers/dates`) mine evidence strings. They also run as defence in depth on token-stripped text after the leak check.
5. Item citations: in reports, every position token (`move`/`best`/`pv`) must reference one of the item's `citations` (`uncited_reference`). In chat, position tokens are themselves the citations.
6. Item text is capped at `ITEM_TEXT_MAX_CHARS = 2000` client-side (`schema_mismatch` / `too_big`).
7. Owner input: the chat is no longer cache-only for the engine. `get_eval` stays cache-only. The new `analyse_position` runs an interactive engine check under its own protocol, and its results are stored apart from `position_evals`. Free users may chat (3 messages a week).
8. On the Postgres queue, AI unavailability re-queues through `JobError::Release { delay }` from the schedule. The retry count lives on the run or note row. There are no BullMQ `-r<n>` job ids.

### Regex translation table (deviation 2)

| JS | Rust | Why |
|---|---|---|
| `(?<!C)X` (a char-class lookbehind) | `X`, plus `before_ok(prev char)` in `scan` | No lookbehind. A rejected start retries at start + 1, and the scan skips every position whose before-check fails, so a run of rejections costs one pass. |
| `X(?!C)` at the end, with a fixed match end | `X`, plus `after_ok(next char)` (ISO dates, `double`, `half`) | No backtracking choice can change the end, so a post-check is exact. **`after_ok` is valid only for fixed-end patterns.** On a variable-length pattern it rejects a match that JS would shorten by backtracking: JS `[a-c]+(?!\d)` matches "ab" in "abc1", but the post-check rejects "abc". Such patterns must use the next row. |
| `X(?!C)` at the end, where `X` has optional or alternative parts | `(?P<m>X)(?:[^C]\|$)`. The logical match is `m`, and the next search resumes at the end of `m`. | Leftmost-first semantics then pick the same shorter match that JS backtracking finds (`0-0-0x` gives `O-O-0x`, and `e7-e8=Qx` gives `e7e8`). |
| An inner lookahead (`\d{1,3}(?:,\d{3})+(?!\d)`, `(?![\d:])`, and the year tail `YEAR(?![A-Za-z0-9])`) | Folded into a consumed alternative (`(?:\.(f)\|[^0-9]\|$)`), or a small manual matcher that tries the JS backtracking order (`mdy_day`, `year_after`) | Same result, and still linear. |
| `(?=PH)` | `…\x{1}`, consumed, with only `m` replaced | Same. |
| `\d` (no `u` flag) | `[0-9]` | Rust `\d` is Unicode. The class is ASCII in both, but hardening H1 folds every `\p{Nd}` digit to ASCII first. So `extract_numbers("٣ games")` is `[]` in TS and `[3]` in Rust, deliberately. |
| `\s` | `[\t\n\x0B\x0C\r \xA0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000\uFEFF]` (`JS_SPACE`) | Rust `\s` adds U+0085 and lacks U+FEFF. `trim` and `split(/\s+/)` use the same set. |
| `\b` | `(?-u:\b)` | JS `\b` is ASCII even with the `i` flag. |
| The `i` flag | `(?i:…)` scoped to ASCII words, never around `[A-Za-z]` classes | Unicode folding adds only U+212A (Kelvin) and U+017F (long s) to ASCII letters, and NFKC has already mapped both away. |
| `\p{Nd}` (`u` flag) | `\p{Nd}` | Same class. Unicode versions may differ slightly. |
| `s.normalize("NFKC")` | `unicode_normalization::nfkc`, followed by H2 and H3 | Same algorithm, possibly another Unicode version. |
| UTF-16 indexes (`slice(i - 16, i)`, `PH.repeat(m.length)`, sort by `index`) | `window_before` counts UTF-16 units, `mask` writes one placeholder per unit, and `Utf16Cursor` sorts hits by UTF-16 offset | Window sizes and ordering stay observable and identical. Other offsets are byte offsets. |

Every lookaround in the validator uses one of these forms. A new pattern must use one of them too, and must say which one in a comment.

### Bounded and linear

Each `regex` call is linear in the span it searches, but a sum of linear calls is not automatically linear: the total is what matters. The rules that keep the total linear in the input:
- `scan` restarts after a match at its end. After a rejection it skips every start whose before-check fails, so a run of rejected starts costs one pass.
- Per-candidate checks run only on bounded windows (the 12-, 16- and 20-unit look-behind windows) or on slices that do not overlap across candidates (the text after a number, a word run or "half", up to the next token).
- Nothing rescans a growing prefix. The TS bare-square rule tests `^[gap]*$` on `slice(lastEnd, c.at)`, which rescans one long gap for every rejected square after it: quadratic. The first version of this port inherited that. `scan_moves` now walks the text once, keeps the last non-gap position, and accepts the gap iff that position is before `last_end`. The result is the same, in linear time.

Two tests in `tests/extract_gaps.rs` enforce these rules:
- `bounded_matches_is_linear_in_rejections` counts the boundary-predicate calls: at most n + 2 on 200k rejected starts.
- `extractors_are_linear_on_large_hostile_text` runs each of 28 hostile shapes through all four functions at N = 16 Ki and at 4N, taking the fastest of three runs. It requires the 4N time to be at most 10× the N time, plus 40 ms. Linear work is about 4× and quadratic about 16×. The quadratic gap shape (an accepted move, a long gap, then many rejected squares) measured 0.48 s against 7.6 s before the fix.

### Deliberate hardening beyond TS (fix round 1, binding ruling)

A missed claim is a leak, so `normalize_text` is deliberately stricter than TS. Each item below is a recorded deviation, and each has a test in `tests/extract_gaps.rs` `mod hardening`.

- **H1. Decimal digits.** Every `\p{Nd}` digit is folded to ASCII after NFKC (`٣` becomes `3`, `१२` becomes `12`). Unicode keeps decimal digits in contiguous runs of ten, so a digit's value is its offset in the run, modulo 10.
- **H2. Invisibles.** `strip_invisibles` removes the whole `\p{Default_Ignorable_Code_Point}` set as well as TS's list. That adds, for example, U+180F, U+206A–206F, U+FFF0–FFF8, U+1BCA0–1BCA3, U+1D173–1D17A and all of U+E0000–E0FFF.
- **H3. Combining marks.** After NFKC, the text goes through NFKD, every `\p{Mn}` is dropped, and the text is recomposed with NFC. So `N\u{301}f3` reads as `Nf3`, and `é` reads as `e`.
- **H4. Homoglyphs.** Cyrillic, Greek and Armenian lookalikes of chess letters (files, pieces, `O`, `x`) and digits are folded to ASCII. Examples are Cyrillic а е о р с х К В Н О Р Т and З→3, Greek Α Β Ε Κ Ν Ο Ρ Τ Χ ο ν, and a few Latin ones (ɡ ɑ ı). They live in one table, `normalize::HOMOGLYPHS`, a superset of TS `leaks.ts`'s `CONFUSABLES` that the leak detector (T6) reuses.
- **H5. Dashes.** The dash set adds U+02D7, U+2E3A, U+2E3B, U+058A and U+30FC to TS's `DASH`, which already holds U+2796. It is one table, `normalize::DASH_RANGES`, used by `is_dash`, by the castling and minus rules, and (through `dash_class()`) by the long-algebraic separator class. TS's class there was only `[-‐-―−x]`.
- **H6. Thousands separators.** U+2006, U+2008 and U+200A group thousands, as U+00A0, U+202F, U+2009 and U+2007 already did.

The pipeline is:
1. strip invisibles;
2. group thousands;
3. NFKC;
4. strip invisibles;
5. H3;
6. H1 and H4, char by char;
7. TS's castling dashes, minus before a digit, figurines and `0-0` castling.

In the differential corpus, the inputs these items affect are listed in `HARDENED`. The test asserts that they do differ from TS, so the list cannot go stale. Every other input must match TS exactly.

### Parity checklist rule

The plan's parity checklist (TS test file → Rust test file → task) is binding:

| TS test file (`.worktrees/m3/packages/ai/test/`) | Rust test file (`backend/crates/ai/tests/`) | Task |
|---|---|---|
| `input/schema.test.ts` | `input_schema.rs` (+ `input_shape.rs`) | T2 (T1) |
| `output/output.test.ts` | `output.rs` | T3 |
| `validator/extract.test.ts` (except the `pvToSan` block) | `extract.rs` | T5 |
| `validator/extract-gaps.test.ts` | `extract_gaps.rs` | T5 |
| `validator/tokens.test.ts` | `tokens.rs` | T6 |
| `validator/leaks.test.ts` (all 56 escapes) | `leaks.rs` | T6 |
| `validator/evidence.test.ts` | `evidence.rs` | T7 |
| `validator/paths.test.ts` | `paths.rs` | T7 |
| `extract.test.ts` `pvToSan` block | `pv.rs` | T7 |
| `cost.test.ts` | `cost.rs` | T9 |
| `client/fix-round-1.test.ts` (parse reasons, taxonomy, custom ids, UnknownModel) | `parse_output.rs`, `client_errors.rs`, `custom_id.rs`, `cost.rs` | T9 |
| `client/anthropic.test.ts`, `fix-round-1.test.ts` (adapter and batch parts) | `http_client.rs` | T11 |
| `client/fake-breaker.test.ts`, `fix-round-1.test.ts` (breaker and fake parts) | `breaker.rs`, `fake_client.rs` | T12 |

The rules for every Rust file:
- it keeps the TS test titles in snake_case (a `describe` becomes a module);
- it asserts the same strings and values;
- a TS test with no Rust equivalent is listed in the plan with its reason (none are planned).

Where a port depends on engine semantics, a recorded differential corpus pins it. The validator's corpus has 163 inputs. They cover:
- backtracking;
- inner lookaheads;
- the members of JS `\s` (U+0085, U+1680, `\t`, `\v`, U+2029);
- astral characters split by the 12-, 16- and 20-unit windows;
- `Oct 14th, 2026`;
- the hardening inputs.

`backend/tools/gen-validator-corpus.mjs` generates it. The tool runs m3's `normalize.ts` and `extract.ts` under Node type stripping and rewrites the `BEGIN GENERATED` block of `tests/extract_gaps.rs`. That block records the Node, ICU and Unicode versions and the m3 commit (last run: Node 22.22.0, ICU 77.1, Unicode 16.0, m3 `4ff8220`). The output is deterministic, so re-running the tool on the same versions leaves the file unchanged. The block is checked by `ts_parity::rust_matches_the_recorded_ts_output_on_the_edge_corpus`. To add a case, edit the tool's `inputs` and re-run it. Never hand-edit the block.

## Consequences

- The extractors and the normaliser behave as TS does on the ported tests and on the corpus, except for the deliberate hardening H1–H6.
- Linear time is a property of the rules above, not a guarantee of the `regex` crate alone. It is enforced by the scaling test, and every new check must keep to bounded windows or non-overlapping slices.
- The lookaround forms above are the only sanctioned ones. Tasks 6–8 (tokens, leaks, claims) reuse `bounded::scan` and the same constants. The leak detector should reuse `HOMOGLYPHS` and `DASH_RANGES`, not copy TS's smaller tables.
- Unicode-version drift between ICU and `unicode-normalization` or `regex` is accepted. Such characters are rare in model text, and the leak detector catches the cases that matter.
