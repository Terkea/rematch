# ADR-R3-4. Sectioned generation and facts by reference

Date: 2026-11-21
Status: Accepted ("Schemas" and "Computed focuses and drills", R3 Task 3; "Claim validator" and "Language rules", R3 Task 8). Task 14 appends its section. Placeholder number; Task 26 renumbers it. Ports ADR-AI-7 (TS M3) to the Rust `ai` crate.

## Context

Claude writes the report narrative in five section calls, plus weekly reviews and change notes. The same schema reaches Claude by two paths (ADR-R3-6):

- Interactive calls go through the Agent SDK sidecar as `outputFormat: {"type":"json_schema","schema":…}`, which the SDK reads as JSON Schema draft-07.
- Batches (Tasks 16 and 20) go to the Messages API as `output_config.format`, which applies the structured-output subset. That subset rejects recursive schemas, numeric and string constraints and complex array bounds, and it needs `additionalProperties: false` on every object.

Each output contract must therefore be valid under both. The guard below enforces the intersection of the two: keywords that mean the same in draft-07 and draft 2020-12, inside the structured-output subset. The TS reference got its schemas from `zodOutputFormat`, which strips the unsupported keywords, restates them as description hints and validates them client-side. The Rust crate does the same in `ai::output::schema`. The contract types also have to match the TS shape the web reads.

## Schemas

- **One envelope for every section.** `ReportSectionEnvelope { section, overview, weaknesses, openings, climbPlan, areas }` serves all five section calls, so the output format and the cached prefix are byte-identical. Every part is present-but-nullable: the key must be on the wire, and it is `null` when it is not the section asked for. The weekly review (`WeeklyReviewOutput`) and the change note (`WeaknessChangeOutput { text, citations }`) have their own schemas. Ids: `report-section`, `weekly-review` and `weakness-change`, all version 1, resolved by `schema_by_id` (batch custom ids carry both).
- **Sanitized schemars output.** `api_schema::<T>()` generates draft 2020-12 with `inline_subschemas = true` (no `$defs` and no `$ref`). It applies R2's `contract_transform`, the single mechanism for required plus nullable (every property is required, and an `Option` stays `T | null`), so the crate has no per-field schema wrappers. Then `sanitize` drops `$schema`, `title`, `format`, `minLength`, `maxLength`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `pattern`, `minItems`, `maxItems` and `default`. `$schema` is dropped deliberately: the schema is sent to a draft-07 reader (the SDK) and to the Messages subset, so it must not name either dialect. It rewrites `type: [X, "null"]` as `anyOf: [{…X}, {"type":"null"}]` and sets `additionalProperties: false` on every object. It also drops `type` beside `enum`, because the enum already fixes the type and the zod export has no `type` there. The dropped `minLength`, `maxLength`, `minItems` and `maxItems` are restated at the end of the node's description, as the TS SDK does (`{minLength: 1, maxLength: 2000}`, `{maxItems: 4}`, `{maxItems: 3}`, `{minItems: 1, maxItems: 3}`), so the model still sees them. Numeric bounds such as schemars' `minimum: 0` on a `u32` are dropped without a hint. The rewrite descends only through schema positions (`properties`, `items`, the unions and `$defs`), so a property named like a keyword (`title`) survives.
- **Bounds are checked client-side.** Each `OutputSchema.check` first deserialises strictly with `serde_path_to_error`. Shape errors give one `ParseIssue` with a dotted path and a code (`invalid_type`, `unknown_field`, `missing_field` or `invalid_value`), and never the model's text. An unknown key is reported at its parent object, as zod does, so a key name the model chose never enters a path. The check then enforces what the API cannot: every item text is 1 to `ITEM_TEXT_MAX_CHARS = 2000` code points (`too_small` / `too_big`), with at most 4 strengths, at most 3 focuses, 1 to 3 drills per focus, at most 3 weekly outcomes and at most 2 weekly exemplars. A failure is `ParseError::SchemaMismatch` with those issues.
- **The guard is ref-free and strict.** `assert_api_subset` walks every node. It allows only `type` (one basic type), `properties`, `required` (exactly the property names), `additionalProperties` (only `false`, and required on objects), `items`, `enum`, `const`, `anyOf` and `description`. That is narrower than what the API accepts, because `$ref`, `allOf` and string formats are excluded too, so a new contract cannot drift towards the edge of the subset. Tests run the guard on all three schemas and compare the envelope's shape with the TS export (`fixtures/ai/ts-export/report-section-envelope.schema.json`) using R2's shape walker.
- **Descriptions are model-facing, and only deliberate.** A `description` in an output schema is text the model reads. Doc comments on output types would become descriptions, so those types carry `//` comments instead. Model-facing text is set only with `#[schemars(description = …)]` from the constants in `schema::desc`. A field-level description also replaces the doc text of a foreign type (R2's `AreaId` and `Verdict`). `schema_descriptions_are_pinned` asserts the exact set of descriptions in each schema, hints included, so a stray doc comment fails the test.
- **Validated sections keep the TS shape.** `ValidatedSection` is a struct with the five camelCase keys `overview`, `weaknesses`, `openings`, `climbPlan` and `areas`. Every key is always serialised and nullable, and exactly one is non-null: the one for the section. It is built with `ValidatedSection::new(ValidatedPart::…)`, and deserialising rejects zero or several non-null parts. This is exact parity with the TS `ValidatedSection` that the web reads.
- **Deviation from the TS export.** The SDK keeps `minItems: 1` on `drills`, which the API accepts. We drop every array bound and check `drills` client-side instead, so the shape test removes that one keyword from the TS side before comparing.

## Computed focuses and drills

Carried over from ADR-AI-7:

- Deterministic code computes or catalogues the focuses (which weaknesses to work on, at most 3), the opening verdicts (keep, fix, learn, replace) and the drill types. The model does not choose them.
- The drill catalogue `drills_for(weakness_id, variant)` (`output/drills.rs`, a port of `drills.ts`, improvement-engine §6.4) lists the allowed `DrillType`s for each weakness. It has 18 snake_case values. Unknown ids give an empty list. `W3` with variant `deep` gives `long_calculation` only. Every id except `W12` adds `own_game_analysis`.
- Claude only narrates, and for each selected focus it picks 1 to 3 drill types from the allowed list. The validator (Task 8) rejects anything else with `focus_not_selected`, `drill_not_allowed` or `verdict_mismatch`.
- Doses (how many drills, how often) belong to Milestone 6.

## Claim validator

`validator::text::validate_text(text, citations, evidence, rules, path)` decides whether one paragraph is safe to show. On success it returns the server-rendered segments. On failure it returns the errors of the first failing stage. Every error rejects the paragraph, so stopping at the first failing stage never lets anything through; it only keeps a later stage from restating an earlier failure (a free-text `Qxf7#` is a leak and also an extracted move).

**The check order.**
1. Citations: each is a key of the evidence `positions` (`unknown_citation`). A rule set that requires a citation, with none, gives `missing_citation`. Duplicates are fine.
2. Token syntax (`token_syntax`): any `ParsedTokens.errors` entry rejects. Prose inside a malformed `{{…}}` is never leak-scanned, so a malformed token cannot be tolerated. `{{offtopic}}` is valid only as the whole paragraph; anywhere else it is an error. As the whole paragraph it reaches stage 5 and does not resolve, because report text cannot be off-topic; the chat engine handles the marker before it validates. The detail holds the message and the raw token, never an index: token indexes are offsets in the canonical text, and stray-brace indexes are offsets in `stripped`.
3. Leaks: `find_leaks(stripped)` (`free_text_fact`). The detail lists `kind:"text"` pairs.
4. Defence in depth: `extract_moves`, `extract_numbers` and `extract_dates` on the stripped text (`unknown_move`, `unknown_number`, `unknown_date`). With facts by reference, any result at all is a fact outside a token, so nothing is compared with the evidence. On the fixtures and on the corpus, stage 3 always fires first; this stage covers a gap in the leak detector. It can reject harmless prose ("half the games" is a ratio claim), which costs a repair.
5. Every token resolves (`unresolved_token`). This catches unknown positions, paths, line keys and eval ids, out-of-range `pv`/`line` indexes, and numbers `format_number` cannot print. The paragraph must then render (`render_segments`).
6. Unless `implicit_citations` (chat), every `move`/`best`/`pv` token's `gameId#ply` is one of the item's citations (`uncited_reference`).
7. With `line_keys: Some(keys)`, `line` tokens are allowed only for those keys (`unknown_line`). `None` allows any line the evidence knows.
8. The `extra` rules (the language rules), on the stripped text, with the tokens and citations in `TextCtx`.

**Why the leak detector replaced free-text matching.** TS M3 extracted moves, numbers and dates from free text and compared them with the evidence, with tolerances, hedges and ratio windows. Each tolerance was a way to smuggle in a plausible but wrong fact, and every Unicode trick (full-width digits, homoglyphs, figurines, zero-width characters) was a way to hide one from the extractor. With facts by reference the model never states a fact. The leak detector rejects anything that looks like one: a digit in any script, a number word, a square, SAN, castling, a date form or `%`. Both the detector and the extractors run the TS-exact and the hardened normalisation and take the union (ADR-R3-2). `normalize_text` alone is hardened only, and it is never used for a check.

**Token scoping to citations.** A report item cites positions in `citations`, and its position tokens may only point at those. So a weakness item cannot quote a move from another weakness's exemplar, and an item without citations (a strength, an area) cannot use position tokens at all. The `line` tokens of an opening item must name that item's line. In chat, the tokens are the citations.

**Sections.** `validate_section` keeps TS plan A4's rules.
- Fatal errors drop the section, which then renders computed-only: `wrong_section` (another section, or the requested part is null), `too_many_focuses` (more than 3) and `area_missing` (not exactly the 7 distinct areas).
- Item errors drop one item. A singleton becomes `None`, and each drop is listed in `item_errors` with its path, such as `weaknesses.items[1]`. The item codes are:
  - `unknown_weakness`;
  - `unknown_line`, and `verdict_mismatch` against the computed verdict;
  - `focus_not_selected`: the id is not in `select_focuses`, or it is repeated. With no selected focus, only `general` is allowed;
  - `drill_not_allowed`: not in `drills_for(id, variant)`. The general focus allows only `own_game_analysis`, the drill that every weakness except W12 allows.
- Weakness, focus (`why`) and opening items need a citation; strengths and areas may have none.
- Model text outside an `Item` is shown too. A weakness `title` and a drill `rationale` are therefore validated with the `Item` rules, and they may not hold tokens, because they carry no segments. A failing title drops its weakness, and a failing rationale drops its focus.
- `select_focuses` returns ids by ascending rank with confidence ≥ 0.7 and state active, emerging, relapsed or improving, never W12. It returns at most 2 on the first report and 3 otherwise.

**Repair once, then drop.** The generator (Task 14) sends the errors of a failed section or item back once, as a repair turn. If the repaired output still fails, the item or section is dropped and never shown, and the report renders the computed data in its place.

**Known limit: a token's value is not bound to its meaning.** `{{num:basis.importedGames}}` renders a true number, but the prose around it may call it the wrong thing ("you lost {{num:basis.importedGames}} games"). The validator checks that every fact is real, not that the sentence uses it correctly. The evidence-window and needMore rules check for specific paths, which narrows the gap for the claims that matter most. The prompts (Task 13) state what each path means.

**Differential corpus.** TS M3 has no token-era claim validator. So `backend/tools/gen-claim-corpus.mjs` holds the TS reference: the check order above, run on m3's own `parseTokens`, `findLeaks`, `normalizeText` with the extractors, and `buildEvidence` with `resolveToken`, plus the language rules as the TS plan's JS regexes. It records the codes TS gives for 153 paragraphs in `tests/validator_adversarial.rs`: the 56 leak escapes in context, the adversarial cases, token edges and every language rule. `ts_parity::rust_rejects_everything_ts_rejects` asserts superset safety on every row (TS rejects ⇒ Rust rejects). It also asserts equal deduplicated codes on every row except the listed deviations:
- `HARDENED`: TS accepts and Rust rejects, through the union below;
- `DIFFERS`: both reject, with different codes. `{{offtopic}}` and `{{eng:}}` are TS syntax errors and Rust `unresolved_token`s.

Both lists must stay accurate, and the generator's output is deterministic.

**Cost.** Every pass reports to `count_scanned`. `text::tests::scanned_bytes_are_linear_on_hostile_shapes` bounds the whole check, language rules included, at 200·n.

## Language rules

The rules are `TextRule`s over the stripped text (each token is `§`), in `validator::language`. The patterns are the TS plan A5 regexes. JS `/i` without the `u` flag folds only ASCII letters, and JS `\b` is ASCII, so each pattern is written `(?i-u:…)` and matches what JS matches.

| Rule | Code | Token form |
|---|---|---|
| `banned_phrases()`: `guarantee[ds]?`, `definitely`, `certainly`, `proven?`, `will (gain\|reach\|get you to\|raise your rating\|improve your rating)` | `banned_phrase` | as TS |
| `no_marketing()`: `upgrade`, `subscribe`, `subscription`, `premium`, `pro plan`, `discount`, `free trial`, `refer a friend`, `share your (report\|card)` | `marketing` | as TS |
| `likelihood(required)`: any vocabulary phrase other than `required`, longest first with word boundaries (so `unlikely` is not a phrase) | `likelihood_mismatch` | as TS; `None` makes every phrase a mismatch |
| `evidence_window(tc)` | `missing_evidence_window` | a `{{num:basis.analysedGames…}}` token and the time-class word (TS: the count as digits) |
| `no_causal_claims()`: `caused`, `because of`, `due to`, `the reason (is\|was)`, `led to`, `proves?`; plus a required hedge (`may`, `might`, `possibly`, `could`, `coincid…`, `around the time`, `at about the same time`) | `causal_claim`, `missing_hedge` | as TS |
| `states_insufficient_data(area)` | `insufficient_data_not_stated` | a `{{num:areaScores.<area>.needMore…}}` token (TS: the count as digits) |

- `likelihood_phrase(p_imp)` gives "very likely" at ≥ 0.95, "likely" at ≥ 0.85, "unclear yet" at ≥ 0.5, and "no clear change yet" otherwise. It gives `None` when there is no `p_imp`.
- `confidence_phrase(c)` gives "clearly" at ≥ 0.9, "probably" at ≥ 0.7, and "possibly" otherwise.
- `evidence_window_token(tc)` is the token form that prompts show the model.
- `format_evidence_window(basis, tc)` ("over your last 214 analysed rapid games (3,912 imported)") is for computed text only, never for model text.

**The union rule.** A rule that forbids a phrase fires if the phrase is in the stripped text or in its hardened normalisation, where homoglyphs, combining marks and every default-ignorable code point are folded away. So `guаranteed` with a Cyrillic `а`, or `Up` U+180F `grade`, is still caught; TS lets both through. A rule that requires a phrase (a hedge, the time-class word) reads only the stripped text, the form TS reads, because folding could only make a missing phrase appear. A hedge disguised with a homoglyph does not count.

**Which rules apply** (`report_text_rules(input)`):
- `banned_phrases` and `no_marketing` apply everywhere.
- Weakness and focus (`why`) texts add a required citation, `likelihood(likelihood_phrase(w.p_imp))` and `evidence_window`.
- The summary adds `evidence_window`.
- Area texts for an area below its minimum sample add `states_insufficient_data`.
- Opening items add a required citation and their own line key.
- `no_causal_claims` is for change notes (Task 14).

Marketing is banned for every user, not only minors, because the report cannot know who reads it (spec §14).

## Consequences

- The model cannot promote a weakness that was not selected, override a verdict or invent a drill. Changing the catalogue is a code change with tests.
- Adding a field to an output type changes its API schema automatically. The guard and the shape test fail if the new field leaves the subset or drifts from the TS contract.
- Bounds are enforced in Rust code. The schema carries them only as description hints, so the model may still emit an over-long text or array. That output is a schema mismatch and goes through the usual repair-once-then-drop path.
- The validator rejects any paragraph that states a fact outside a token, cites a position that was not supplied, uses a token it cannot resolve or did not cite, or breaks a language rule. On the recorded corpus it rejects everything the TS reference rejects, so a Rust change that lets a TS-rejected paragraph through fails a test.
