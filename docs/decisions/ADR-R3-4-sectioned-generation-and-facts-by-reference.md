# ADR-R3-4. Sectioned generation and facts by reference

Date: 2026-11-21
Status: Accepted ("Schemas" and "Computed focuses and drills", R3 Task 3; "Claim validator" and "Language rules", R3 Task 8 and its fix round 1). Task 14 appends its section. Placeholder number; Task 26 renumbers it. Ports ADR-AI-7 (TS M3) to the Rust `ai` crate.

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

`validator::text::validate_text(text, citations, evidence, rules, path)` decides whether one paragraph is safe to show. On success it returns the server-rendered segments. On failure it returns every error it found, sorted by a stable key (the code's declaration order, then the path, then the detail). Any error rejects the paragraph. Callers pass schema-checked text of at most `ITEM_TEXT_MAX_CHARS` code points, which a debug assertion checks; the cost is linear either way.

**The check order.**
1. **Citations.** Each citation must be a key of the evidence `positions` (`unknown_citation`). A rule set that requires a citation, with none given, gives `missing_citation`. Duplicates are fine.
2. **Token syntax** (`token_syntax`).
   - Any `ParsedTokens.errors` entry is an error. `{{offtopic}}` is an error anywhere except as the whole paragraph.
   - **This is the only hard barrier.** Prose inside a malformed `{{…}}` is never leak-scanned, so after a syntax error the result is the errors of stages 1 and 2.
   - The detail holds the message and the raw token, never an index (token indexes are in the canonical text, stray-brace indexes in `stripped`).
   - As the whole paragraph, `{{offtopic}}` reaches stage 5 and does not resolve: report text cannot be off-topic, and the chat engine handles the marker before it validates.
3. **Leaks.** `find_leaks(stripped)` gives `free_text_fact`, and the detail lists `kind:"text"` pairs.
4. **Defence in depth, only when stage 3 found nothing.** The union extractors run on the stripped text (`unknown_move`, `unknown_number`, `unknown_date`). Any result at all is a fact outside a token. On the fixtures and the corpus, stage 3 always fires first, so this stage covers a gap in the leak detector.
5. **Tokens** (fix round 1, C1 and C2).
   - Every token resolves; otherwise `unresolved_token`. This catches unknown positions, paths, line keys and eval ids, out-of-range `pv`/`line` indexes, and numbers `format_number` cannot print.
   - A `num` token's format must suit its value and path (`num_format_allowed`); otherwise `unresolved_token`:
     - `pct0`/`pct1` only for a rate or share path (`Evidence::rate_paths`: weakness rates, intervals, band medians, confidence and `pImp`; change rates and `posteriorProb`; opening scores and alternative band scores; metrics with unit `rate`) holding a value in [0, 1];
     - `int` and no format only for whole numbers;
     - `signed1` only for an evaluation in pawns (`….eval*.pawns`);
     - `dec1`/`dec2` for any value.
   - No token is glued (`glued_token`). A token directly next to a letter, a digit, one of `x # + = - / % ½ ×`, any dash, or another token is glued. So is a `.` or `,` between a token and a letter, a digit or another token (`{{num}}.{{num}}`, `{{num}},5`).
     - Invisible characters and combining marks between them do not separate them, so `N` U+180F `{{sq:e5}}` is glued.
     - Sentence punctuation (`{{move}}.` followed by a space or the end of the text) is not glue. Rejecting it would reject almost every sentence that ends on a token, and it cannot build notation.
6. **Scoping to the cited positions** (`uncited_reference`).
   - Unless `implicit_citations` is set (chat), every `move`/`best`/`pv` token's `gameId#ply` must be one of the citations.
   - Every `sq` token must name a square of a cited position's recorded moves or engine lines: the from and to squares of its allowed UCI, and the target squares of its allowed SAN. In chat, the position tokens of the paragraph count as its citations.
7. **Line keys.** With `line_keys: Some(keys)`, `line` tokens are allowed only for those keys (`unknown_line`). `None` allows any line the evidence knows.
8. **The `extra` rules** (the language rules). They run on the stripped text, with the tokens, citations, canonical text and the paragraph's single hardened normalisation in `TextCtx`.

Stages 1 and 2 are always reported together. Without a syntax error, stages 3, 5, 6, 7 and 8 all run and all their errors are reported (stage 4 only when stage 3 was empty), so a repair turn sees every problem at once.

**Why the leak detector replaced free-text matching.** TS M3 extracted moves, numbers and dates from free text and compared them with the evidence, using tolerances, hedges and ratio windows. Each tolerance was a way to smuggle in a plausible but wrong fact, and every Unicode trick (full-width digits, homoglyphs, figurines, zero-width characters) was a way to hide one from the extractor. With facts by reference the model never states a fact. The leak detector rejects anything that looks like one: a digit in any script, a number word, a square, SAN, castling, a date form or `%`. Both the detector and the extractors run the TS-exact and the hardened normalisation and take the union (ADR-R3-2). `normalize_text` alone is hardened only, and it is never used for a fact check.

**Citations are tied to the item's subject** (fix round 1, C3, `unrelated_citation`). A report item cites positions in `citations`, and its position and square tokens may only point at those. The citations themselves must belong to what the item narrates:
- a weakness item cites only that weakness's exemplars;
- a focus `why` cites its weakness's exemplars (the general focus may cite any supplied position);
- an opening item cites only its line's examples;
- a strength cites only the exemplars of the computed strengths in its area.

Area texts and the singleton items (headline, summary, intro, habit, weekly rhythm) are not tied to a subject, but their tokens are still scoped to their own citations. An item without citations cannot use position or square tokens at all. The `line` tokens of an opening item must name that item's line. In chat, the tokens are the citations.

**Sections.** `validate_section` keeps TS plan A4's rules.
- **Fatal errors** drop the section, which then renders computed-only:
  - `wrong_section`: another section, or the requested part is null;
  - `too_many_focuses`: more than 3;
  - `area_missing`: not exactly the 7 distinct areas.
- **Item errors** drop one item. A singleton becomes `None`, and each drop is listed in `item_errors` with its path, such as `weaknesses.items[1]`. The item codes are:
  - `unknown_weakness`, also for a second item on a weakness whose item was already accepted;
  - `unknown_line` (also for a duplicate opening item), and `verdict_mismatch` against the computed verdict;
  - `focus_not_selected`: the id is not in `select_focuses`, or it is repeated. With no selected focus, only `general` is allowed;
  - `drill_not_allowed`: not in `drills_for(id, variant)`. The general focus allows only `own_game_analysis`, the drill that every weakness except W12 allows;
  - `area_missing` for a strength in an area with no computed strength;
  - `unrelated_citation`, as above.
- A focus whose `why` fails is dropped whole.
- Weakness, focus (`why`) and opening items need a citation; strengths and areas may have none.
- A weakness item's `title` is the computed weakness label; the model's `title` is ignored. A drill `rationale` is model text that is shown, so it is validated with the `Item` rules and may not hold tokens, because it carries no segments.
- `select_focuses` returns ids by ascending rank with confidence ≥ 0.7 and state active, emerging, relapsed or improving, never W12. It returns at most 2 on the first report and 3 otherwise.

**Repair once, then drop.** The generator (Task 14) sends the errors of a failed section or item back once, as a repair turn. If the repaired output still fails, the item or section is dropped and never shown, and the report renders the computed data in its place.

**Known limit: a token's value is not bound to its meaning.** `{{num:basis.importedGames}}` renders a true number, but the prose around it may call it the wrong thing ("you lost {{num:basis.importedGames}} games"). The validator checks that every fact is real, not that the sentence uses it correctly. Fix round 1 narrows the gap:
- formats are bound to the kind of value;
- citations are bound to the item's subject;
- engine words need engine evidence;
- quantities in words are banned;
- the evidence-window and needMore rules check for specific paths.

The prompts (Task 13) state what each path means.

**Leaf-module parity check (the TS corpus).** TS M3 has no token-era claim validator. `backend/tools/gen-claim-corpus.mjs` therefore composes a TS reference from m3's leaf modules (`parseTokens`, `findLeaks`, `normalizeText` with the extractors, and `buildEvidence` with `resolveToken`) plus the TS plan's language regexes. It records the verdict TS gives for 153 paragraphs in `tests/validator_adversarial.rs`: the 56 leak escapes in context, the adversarial cases, token edges and every language rule.

It is a parity check of those leaf modules and regexes, not of a TS validator. `ts_parity::rust_rejects_everything_ts_rejects` asserts superset safety on every row: if TS rejects a row, Rust rejects it. The rows only Rust rejects must be exactly `STRICTER`: the hardened-form union, and square scoping. Codes are not compared, because Rust collects more stages and has rules TS lacks.

**Rust-only vectors.** `must_reject::every_review_vector_is_rejected_with_its_code` holds every false-accept vector of the Task 8 review (C1–C6 and the Important items, plus the fix round 2 vectors), 75 paragraphs, each with the code it must give. `must_reject::the_supported_forms_pass` pins the legitimate forms next to them: sentence punctuation after a token, squares of the cited position, an engine word with a `best` token or a cited eval, and bound formats.

**Cost.** Every pass reports to `count_scanned`. `text::tests::scanned_bytes_are_linear_on_hostile_shapes` bounds the whole check at 200·n, including the adjacency scan, the language rules and the blunder windows.

## Language rules

The rules are `TextRule`s over the stripped text (each token is `§`), in `validator::language`. The patterns are the TS plan A5 regexes plus the fix round 1 additions. They are written JS-style and translated once:
- `\b` becomes the ASCII `(?-u:\b)`;
- `\s` becomes JS's whitespace set, and every multi-word phrase allows `\s+` between its words;
- `\w` becomes `[A-Za-z0-9_]`;
- the whole pattern is wrapped in `(?i:…)`. Unicode case folding adds only U+212A and U+017F to ASCII letters, and NFKC has removed both from the canonical text, so this matches what JS `/i` matches.

| Rule | Code | What it checks |
|---|---|---|
| `banned_phrases()` | `banned_phrase` | `guarantee\w*`, `definitely`, `certain(ty\|ly)?`, `proven?`, near-certainty (`almost surely/certain(ly)`, `virtually certain/guaranteed`, `surely`, `no doubt`, `without (a) doubt`, `bound to`, `sure to`), and promised gains: `(will\|'ll\|’ll\|is/are going to) (gain\|reach\|get you to\|raise your rating\|improve your rating)` |
| `no_marketing()` | `marketing` | `upgrade`, `subscribe`, `subscription`, `premium`, `pro (plan\|tier\|version\|account)`, `discount`, `free trial`, `refer a friend`, `share your (report\|card)`, `buy`, `unlock`, `pricing`, `paywall` |
| `likelihood(required)` | `likelihood_mismatch` | Any vocabulary phrase other than `required`, longest first with word boundaries. `(highly\|most\|extremely\|very)[\s-]+likely` are phrases of their own; spaces and hyphens are normalised before comparing. A direction of change (`improving`, `improved`, `getting better/worse`, `fixed`, `worse`, `worsening`, `declining`, `declined`, and since fix round 2 `improvement(s)` and `stopped`) needs the required phrase in the same text. With `None`, every phrase and every direction word is a mismatch. |
| `evidence_window(tc)` | `missing_evidence_window` | A `{{num:basis.analysedGames}}` or `{{num:basis.analysedGames\|int}}` token, and the time-class word |
| `engine_claims()` | `uncited_reference` | `engine`, `stockfish`, `winning`, `losing`, `(check)mate(s/d)`, `advantage`, `equal`, `top choice`, `best move` or `threw away` need, in the same paragraph, a `best`, `pv` or `eng` token, or a `num` token on an evaluation path of a cited position (`Evidence::position_of_path`). Separately, `blunder…` or `mistake…` within four words of a `best` token, or of a `pv` token of the best line, is rejected. |
| `no_quantity_words()` | `free_text_fact` | `most (of your) games`, `the majority`, `nearly every`, `every (single) game`, `each game`, `a handful`, `a score of`, `\w*fold`, `nil`, `always`, `never` |
| `no_causal_claims()` | `causal_claim`, `missing_hedge` | `caused`, `because` (also bare, since fix round 2), `because of`, `due to`, `the reason (is\|was)`, `led to`, `proves?`, plus a required hedge (`may`, `might`, `possibly`, `could`, `coincid…`, `around the time`, `at about the same time`); for change notes |
| `no_causal_phrases()` | `causal_claim` | The causal phrases alone, with no hedge requirement |
| `states_insufficient_data(area)` | `insufficient_data_not_stated` | A `{{num:areaScores.<area>.needMore…}}` token |

The phrase helpers:
- `likelihood_phrase(p_imp)` gives "very likely" at ≥ 0.95, "likely" at ≥ 0.85, "unclear yet" at ≥ 0.5, and "no clear change yet" otherwise. It gives `None` when there is no `p_imp`.
- `confidence_phrase(c)` gives "clearly" at ≥ 0.9, "probably" at ≥ 0.7, and "possibly" otherwise.
- `evidence_window_token(tc)` is the token form that prompts show the model.
- `format_evidence_window(basis, tc)` ("over your last 214 analysed rapid games (3,912 imported)") is for computed text only, never for model text.

**The union rule.** A rule that forbids a phrase fires if the phrase is in the stripped text or in its hardened normalisation, where homoglyphs, combining marks and every default-ignorable code point are folded away. The hardened form is computed once per paragraph. So `guаranteed` with a Cyrillic `а`, or `Up` U+180F `grade`, is still caught, where TS lets both through. A rule that requires a phrase (a hedge, the time-class word, the likelihood phrase before a direction word) reads only the stripped text, the form TS reads, because folding could only make a missing phrase appear. A hedge disguised with a homoglyph does not count.

**Which rules apply** (`report_text_rules(input)`):

| Kind | Rules |
|---|---|
| Every kind | `banned_phrases`, `no_marketing`, `engine_claims`, `no_quantity_words` (on every kind since fix round 2: strengths, headline, habit, plan and opening text too) |
| Weakness, Focus (`why`) | a required citation, `likelihood(likelihood_phrase(w.p_imp))`, `evidence_window`, `no_causal_phrases` |
| Summary | `evidence_window`, `no_causal_phrases` |
| Area | `states_insufficient_data` below the area's minimum sample |
| Item (headline, strength, intro, habit, weekly rhythm, drill rationale) | `no_causal_phrases` |
| Opening | a required citation, and `line` tokens only for its own line |

`no_causal_claims`, which also requires a hedge, is for change notes (Task 14).

**Rulings of fix round 2.** "improvement" and "stopped" assert a change, so in weakness and focus texts they need the required likelihood phrase, like the other direction words. Bare "because" states a cause, so it is banned wherever the causal ban applies.

**Accepted false positives.** These rules reject some honest prose, which costs a repair turn. They fail closed: the item is dropped, never shown.
- `always`/`never` now reject habits such as "always check captures";
- `buy` also rejects "buy time";
- `certain` rejects "a certain line";
- `mate` rejects a rationale for `mate_patterns`;
- `losing` rejects "losing on time" without an eval token;
- "{{move}} was a blunder; {{best}} was better" trips the four-word window.

Prompts (Task 13) should steer around them.

Marketing is banned for every user, not only minors, because the report cannot know who reads it (spec §14).

## Prompts and the cached request layout (Task 13)

- **Versioned, embedded prompts.** Each prompt is `backend/crates/ai/prompts/<id>.v<n>.md`, embedded with `include_str!` and registered in `prompts::PROMPTS`. A wording change is a new version file, never an edit in place. Each prompt's sha256 is computed once. `prompt_set_hash(REPORT_PROMPT_SET)` is the sha256 over the ordered file hashes, and the report stores it with `REPORT_PROMPT_VERSION = "report@1"`. A test keeps the table and the files on disk equal, and prompt files are LF only, so a hash does not depend on the platform.
- **Templates.** A placeholder is `{{name}}` with `[a-z_]+`. A missing or unused variable is an error. Values are inserted in one pass and never re-scanned. Reference tokens contain `:` and are never placeholders. A chat prompt that teaches `{{offtopic}}` would need an exception, because that token matches the placeholder pattern.
- **Request layout.** `system[0]` is `rules_block()`, which is `core/rules` + `core/tokens`. It holds the format-neutral rules, the forbidden forms and the token grammar, and it says nothing about the output format. Report sections and the chat both send these exact bytes, so the block caches for both (T23 review ruling, fix round 1, which split the old `core/system`). `system[1]` is `report/report` + `report/output` (the JSON output instructions, report only) + `tone/b<n>`, and is shared within a band. The chat adds its own output section and its own repair prompt (T23). A test checks that the shared block mentions no JSON, schema, `citations` or `null`. User block 1 is `<coaching_input>` + the prompt context, shared by the five sections of a report. These are the three breakpoints, and only the uncached section task differs. The sidecar drops the breakpoints (ADR-R3-6) but still benefits from the byte-stable prefix.
- **Canonical JSON.** `canonical_json` matches JS byte for byte: keys are sorted explicitly by UTF-16 code units, and every number is laid out as `Number::toString`, so an integral double prints `48`, `1e21` prints `1e+21` and `-0` prints `0`. `input_hash` is its sha256. A Node-generated golden (`backend/tools/gen-canonical-golden.mjs`) pins the three fixtures and the edge cases. In the prompt, the context goes through `canonical_json_escaped` (`<` written as `\u003c`, the same JSON value), so input text cannot close the tag. Hashes always use the unescaped form.
- **Prompt context.** The input without `accountId`, plus `focuses` (`["general"]` when empty), `evidenceWindow` (the token form), per-weakness `likelihoodPhrase` and `confidencePhrase`, `allowedDrills` per focus (the same rule `validate_section` applies), and `refs {num, date, lines}`.
- **The prompts teach the validator's grammar.** A test renders every prompt and parses every reference token in it. Each concrete example must resolve against the b2-rapid evidence with a format its value allows, and the examples must cover every token kind and every number format. The three before-and-after rewrites in `core/tokens` must be rejected before and accepted after by `validate_text` with the report rules. Every prompt passes `banned_phrases` and `no_marketing`. That is why the language rules in `core/rules` paraphrase the two words those rules ban: the Global Constraints' "never 'guaranteed'" and "never proven". `core/rules` also lists the forbidden forms the validator enforces: "because" and the other causal phrases, quantity words and absolutes ("every game", "always"), number words and ordinals, and words of promise or certainty (described, not spelled out).
- **Fix round 1 (T13 review).**
  - **Model-facing copy passes the validator.** All copy except `core/*` (which names the forbidden forms on purpose) passes `find_leaks`, `no_quantity_words`, `no_causal_phrases`, `engine_claims`, `banned_phrases` and `no_marketing`. Headings are exempt, since a tone heading names its band and rating range. A prompt therefore cannot teach wording that the validator rejects, such as "in every game", "one or two", "half-won" or "first".
  - **Examples are checked in context.** Every reference token in a prompt sits in a backtick span, except the rewrite `After:` lines. Each span, with its schematic `<…>` parts instantiated, passes `validate_text` against a synthetic input: b2-rapid with an insufficient endgame area, so `needMore` resolves. Each span is checked with the positions it refers to cited, and a lone `sq` example cites the prompt's example positions. So squares, glue, paths and formats are checked on every example, including the schematic forms.
- **Repair.** `build_repair_messages` appends the previous assistant content exactly as returned, then the rendered `repair/repair`, with one `- path: code: detail` line per error. Controller ruling, fix round 1: this overrides the brief's `- code: detail`. The path is our own JSON path of the failing part and carries no user data, and the model needs it to repair the right item. `repair/repair` stays report-only, for a complete JSON object. The last message is always the user message, and the repair prompt refers to "your previous output above", which matches the sidecar's `<previous_assistant_output>` flattening.

## Generation (Task 14)

- **Five calls, one envelope.** A report is five structured calls, one per section (`generate::section::generate_section`), instead of one long call. Each section is shown as soon as it passes (progressive UI), is repaired on its own, and reads the shared cached prefix. All five use the one `ReportSectionEnvelope` schema, so the output format, and with it the request prefix, is byte-identical across them.
- **Overview first.** `generate::report::generate_report` runs `overview` alone and awaits it, because its call writes the cache entry of the shared prefix (rules block, report and tone block, the `<coaching_input>` canonical-JSON block). The other non-skipped sections then run in `SECTION_KEYS` order with `REPORT_CONCURRENCY = 2` in flight. `on_section` is awaited per outcome, inside the section's own task. On the first error (a client error, the budget hook, or an `on_section` failure, kept apart as `ReportError::Ai` and `ReportError::OnSection`), no new section starts, the ones in flight finish and are delivered, and the first error is returned. The job resumes with `skip` set to the stored sections.
- **The canonical-JSON input block.** The input goes into the prompt once, as `canonical_json_escaped(build_prompt_context(..))`, so the same input gives the same bytes on every call and on a retry, and `input_hash` names it.
- **Validate, repair once, then drop.** Nothing reaches the payload unvalidated. Attempt 1 is checked by `validate_section`. A result without parsed output is a validation error (`refusal`, `truncated`, else `schema_invalid`, one per parse issue with its path). Any error, fatal or per item, triggers exactly one repair call (`repair_section`, which Task 16 reuses for batch results): the section request plus `build_repair_messages`. When the first result has no content to replay (a refusal), the repair resends the section request alone, because an empty assistant turn is not a valid message. After the repair, items that still fail are dropped and listed with their paths, and the section is `ready` with what passed. A fatal error, an unparsable result or a refusal on attempt 2 drops the whole section (`content: null`, one `DroppedItem` with path `""`), and the web renders that section computed-only. A dropped section never fails the run.
- **The budget hook.** `BeforeCall` runs before every model call, including the repair. A `Budget` error from it stops the call before it is sent and propagates like `Unavailable`. Client errors are never turned into dropped sections, since an outage is retried by the job.
- **Metering.** Every call returns `CallRecord`s, one per `model_usage` entry, with the SDK estimate on the first only (for comparison, never billed). `repair_section` returns only its own call, so Task 16 can mark the batch result `batch: true`.
- **Computed report and payload.** `build_computed_report` copies the engine data (player, basis, area scores, metrics, weaknesses, openings, focuses) and splits the changes into new (`to_state` emerging or active) and fixed (`resolved`). `assemble_report_payload` gives `complete` with all five sections ready, `computed_only` with none, and `partial` otherwise. The disclosure is present exactly when a section is ready. `load_report_payload` builds the same payload from `reports::load_report_rows`. The JSON contract for the web is the snapshot `backend/fixtures/ai/payload/report-payload.example.json` (`schemaVersion: "report-payload-v1"`, camelCase, sections keyed by the snake_case section key). A test fails when the serialised payload differs from it, and `UPDATE_SNAPSHOTS=1` rewrites it.
- **Bounds (T3 review).** `Drill.rationale` (model text that is shown) is capped at 500 code points, and `WeaknessItem.title` (replaced by the computed label, but still model output) at 120. Both are schema hints plus `report_bounds` checks.

## Consequences

- The model cannot promote a weakness that was not selected, override a verdict or invent a drill. Changing the catalogue is a code change with tests.
- Adding a field to an output type changes its API schema automatically. The guard and the shape test fail if the new field leaves the subset or drifts from the TS contract.
- Bounds are enforced in Rust code. The schema carries them only as description hints, so the model may still emit an over-long text or array. That output is a schema mismatch and goes through the usual repair-once-then-drop path.
- The validator rejects any paragraph that states a fact outside a token, cites a position that was not supplied, uses a token it cannot resolve or did not cite, or breaks a language rule. On the recorded corpus it rejects everything the TS reference rejects, so a Rust change that lets a TS-rejected paragraph through fails a test.
