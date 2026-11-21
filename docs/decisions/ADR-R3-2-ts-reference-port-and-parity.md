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
| `X(?!C)` at the end, with a fixed match end | `X`, plus `after_ok(next char)` (ISO dates, `double`, `half`) | No backtracking choice can change the end, so a post-check is exact. |
| `X(?!C)` at the end, where `X` has optional or alternative parts | `(?P<m>X)(?:[^C]\|$)`. The logical match is `m`, and the next search resumes at the end of `m`. | Leftmost-first semantics then pick the same shorter match that JS backtracking finds (`0-0-0x` gives `O-O-0x`, and `e7-e8=Qx` gives `e7e8`). |
| An inner lookahead (`\d{1,3}(?:,\d{3})+(?!\d)`, `(?![\d:])`, and the year tail `YEAR(?![A-Za-z0-9])`) | Folded into a consumed alternative (`(?:\.(f)\|[^0-9]\|$)`), or a small manual matcher that tries the JS backtracking order (`mdy_day`, `year_after`) | Same result, and still linear. |
| `(?=PH)` | `…\x{1}`, consumed, with only `m` replaced | Same. |
| `\d` (no `u` flag) | `[0-9]` | Rust `\d` is Unicode. `extract_numbers("٣ games")` is `[]` in both. |
| `\s` | `[\t\n\x0B\x0C\r \xA0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000\uFEFF]` (`JS_SPACE`) | Rust `\s` adds U+0085 and lacks U+FEFF. `trim` and `split(/\s+/)` use the same set. |
| `\b` | `(?-u:\b)` | JS `\b` is ASCII even with the `i` flag. |
| The `i` flag | `(?i:…)` scoped to ASCII words, never around `[A-Za-z]` classes | Unicode folding adds only U+212A (Kelvin) and U+017F (long s) to ASCII letters, and NFKC has already mapped both away. |
| `\p{Nd}` (`u` flag) | `\p{Nd}` | Same class. Unicode versions may differ slightly. |
| `s.normalize("NFKC")` | `unicode_normalization::nfkc` | Same algorithm, possibly another Unicode version. |
| UTF-16 indexes (`slice(i - 16, i)`, `PH.repeat(m.length)`, sort by `index`) | `window_before` counts UTF-16 units, `mask` writes one placeholder per unit, and `Utf16Cursor` sorts hits by UTF-16 offset | Window sizes and ordering stay observable and identical. Other offsets are byte offsets. |

Every lookaround in the validator uses one of these forms. A new pattern must use one of them too, and must say which one in a comment.

### Bounded and linear

`regex` searches are linear in the haystack. `scan` adds only O(1) work per char on top. Two tests in `tests/extract_gaps.rs` check this:
- `bounded_matches_is_linear_in_rejections` counts the boundary-predicate calls (at most n + 2 on 200k rejected starts);
- `extractors_are_linear_on_large_hostile_text` runs every extractor on 22 hostile 128 KiB inputs (about 9 s in total in a debug build; a quadratic retry would take minutes).

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

Where a port depends on engine semantics, a recorded differential corpus pins it. The corpus is TS output captured with Node and embedded as a literal. An example is `ts_parity::rust_matches_the_recorded_ts_output_on_the_edge_corpus`, with 110 inputs.

## Consequences

- The extractors and the normaliser behave as TS does on the ported tests and on the corpus, and they are linear on any input.
- The lookaround forms above are the only sanctioned ones. Tasks 6–8 (tokens, leaks, claims) reuse `bounded::scan` and the same constants.
- Unicode-version drift between ICU and `unicode-normalization` or `regex` is accepted. Such characters are rare in model text, and the leak detector catches the cases that matter.
