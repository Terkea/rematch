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

The union (below) runs every extractor on both normalisations. That at most doubles the cost, and each path keeps these rules.

Deterministic tests enforce the rules:
- **`extract::tests::scanned_bytes_are_linear_on_hostile_shapes`** (a unit test; binding). A `cfg(test)` counter, `bounded::count_scanned`, records the bytes each regex search in `scan` reads (from the search start to the match end, or to the text end), the bytes skipped after a rejection, and the per-candidate checks in `scan_moves` (the gap walk, the 16-unit window and the gap before the next candidate). `scan` backs `bounded_matches` and every extractor pass. The test runs all four public functions, so both paths, on 31 hostile shapes at n = 32 Ki. It asserts at most 120·n bytes; the worst measured is 67·n, on the gap shape. A quadratic rescan, such as the pre-fix gap check, reads about n²/20 bytes there, which is about 1600·n.
- **`bounded_matches_is_linear_in_rejections`** counts the boundary-predicate calls: at most n + 2 on 200k rejected starts.
- **`extractors_are_linear_on_large_hostile_text`** (timing, N against 4N) is kept as a cross-check. It is `#[cfg_attr(debug_assertions, ignore)]`, so it runs only in release builds. Before the fix, the gap shape took 0.48 s at N and 7.6 s at 4N.

### Deliberate hardening beyond TS (fix round 1, binding ruling)

A missed claim is a leak, so extraction is a **superset of TS by construction (union)**. This is fix round 2's binding ruling.

Each extractor runs one pipeline function twice:
- once with `NormMode::TsExact`, which is TS's steps only, with TS's invisible, dash and thousands sets and TS's long-algebraic class;
- once with `NormMode::Hardened`, which adds H1–H6 below.

The result is the TS-path list unchanged, then each hardened result the TS path did not also find. The difference is a multiset difference by value, in hardened order (`extract::union_by`, linear). `normalize_text` is the hardened normalisation, and `normalize_ts` is TS's.

Fix round 1 applied hardening alone, and that was a regression. Folding a non-ASCII neighbour to ASCII (H1, H3, H4) glues it onto a move or date before the boundary checks run. `Nf3é` becomes `Nf3e`, `e2-e4а` becomes `e2-e4a`, and `14 Octé` becomes `14 Octe`. TS finds all three; hardening alone found none. The union restores them.

Each item below is a recorded deviation. Each has a test in `tests/extract_gaps.rs`, in `mod hardening`; `mod union` tests the merge.

- **H1. Decimal digits.** Every `\p{Nd}` digit is folded to ASCII after NFKC (`٣` becomes `3`, `१२` becomes `12`). The value comes from `ND_ZEROS`, a static table of the code points that start each run of ten (76 runs, generated from Node 22.22, ICU 77.1, Unicode 16.0). `ND_ZEROS` also defines `\p{Nd}` for thousands grouping.
- **H2. Invisibles.** `strip_invisibles` removes the whole `\p{Default_Ignorable_Code_Point}` set as well as TS's list. That adds, for example, U+180F, U+206A–206F, U+FFF0–FFF8, U+1BCA0–1BCA3, U+1D173–1D17A and all of U+E0000–E0FFF.
- **H3. Combining marks.** After NFKC, the text goes through NFKD, every `\p{Mn}` is dropped, and the text is recomposed with NFC. So `N\u{301}f3` reads as `Nf3`, and `é` reads as `e`.
- **H4. Homoglyphs.** Cyrillic, Greek and Armenian lookalikes of chess letters (files, pieces, `O`, `x`) and digits are folded to ASCII. Examples are Cyrillic а е о р с х К В Н О Р Т and З→3, Greek Α Β Ε Κ Ν Ο Ρ Τ Χ ο ν, and a few Latin ones (ɡ ɑ ı). They live in one table, `normalize::HOMOGLYPHS`, a superset of TS `leaks.ts`'s `CONFUSABLES` that the leak detector (T6) reuses.
- **H5. Dashes.** The dash set adds U+02D7, U+2E3A, U+2E3B, U+058A and U+30FC to TS's `DASH`, which already holds U+2796. It is one table, `normalize::DASH_RANGES`, used by `is_dash`, by the castling and minus rules, and (through `dash_class()`) by the long-algebraic separator class. On the TS-exact path the long-algebraic class stays TS's: `-`, U+2010–U+2015, U+2212 and `x`.
- **H6. Thousands separators.** U+2006, U+2008 and U+200A group thousands, as U+00A0, U+202F, U+2009 and U+2007 already did.

The pipeline is:
1. strip invisibles;
2. group thousands;
3. NFKC;
4. strip invisibles;
5. H3;
6. H1 and H4, char by char;
7. TS's castling dashes, minus before a digit, figurines and `0-0` castling.

In the differential corpus, the inputs whose hardened normalisation differs from TS are listed in `HARDENED`. **For every row, each Rust list (numbers, SAN, UCI, dates) must start with TS's list unchanged.** That makes it a superset of TS's, in TS's order, and `normalize_ts` must equal TS's `normalizeText`. A `HARDENED` row may add results but never lose one, and must still differ somewhere, so the list cannot go stale. Every other row must equal TS exactly. The corpus has 175 inputs, including every regression example above.

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

Where a port depends on engine semantics, a recorded differential corpus pins it. The validator's corpus has 175 inputs. They cover:
- backtracking;
- inner lookaheads;
- the members of JS `\s` (U+0085, U+1680, `\t`, `\v`, U+2029);
- astral characters split by the 12-, 16- and 20-unit windows;
- `Oct 14th, 2026`;
- the hardening inputs.

`backend/tools/gen-validator-corpus.mjs` generates it. The tool runs m3's `normalize.ts` and `extract.ts` under Node type stripping and rewrites the `BEGIN GENERATED` block of `tests/extract_gaps.rs`. That block records the Node, ICU and Unicode versions and the m3 commit (last run: Node 22.22.0, ICU 77.1, Unicode 16.0, m3 `4ff8220`). The output is deterministic, so re-running the tool on the same versions leaves the file unchanged. The block is checked by `ts_parity::rust_matches_the_recorded_ts_output_on_the_edge_corpus`. To add a case, edit the tool's `inputs` and re-run it. Never hand-edit the block.

## Consequences

- **Superset by construction.** The extractors return exactly what TS returns (the TS-exact path), plus whatever extra the hardened path finds (H1–H6). They can never lose a TS result, and every corpus row checks this. The hardened `normalize_text` alone is not a superset; only the union is.
- **Cost.** Linear time is a property of the rules above, not a guarantee of the `regex` crate alone. The deterministic byte-count test enforces it on both paths, and the union costs at most 2×. Every new check must keep to bounded windows or non-overlapping slices, and must call `count_scanned`.
- **Lookaround forms.** The forms above are the only sanctioned ones. Tasks 6–8 (tokens, leaks, claims) reuse `bounded::scan` and the same constants. The leak detector should reuse `HOMOGLYPHS` and `DASH_RANGES`, not copy TS's smaller tables.
- **Task 6 (the leak detector) must apply the same union if it reuses `HOMOGLYPHS`** (or any other H-item). It must detect on the TS-exact canonical text and on the hardened one, and flag a leak found by either. Folding alone can glue a neighbour onto a leak and hide it (`Nf3é`). `NormMode`, `normalize_with` and `strip_invisibles_with` exist for this.
- Unicode-version drift between ICU and `unicode-normalization` or `regex` is accepted. Such characters are rare in model text, and the leak detector catches the cases that matter.
