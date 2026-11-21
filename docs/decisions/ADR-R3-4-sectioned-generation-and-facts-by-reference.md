# ADR-R3-4. Sectioned generation and facts by reference

Date: 2026-11-21
Status: Accepted ("Schemas" and "Computed focuses and drills", R3 Task 3). Tasks 8 and 14 append their sections. Placeholder number; Task 26 renumbers it. Ports ADR-AI-7 (TS M3) to the Rust `ai` crate.

## Context

Claude writes the report narrative in five section calls, plus weekly reviews and change notes. Its JSON comes only from `output_config.format` (`{"type":"json_schema","schema":…}`), so every output contract has to be a JSON Schema the API accepts. The structured-output subset rejects recursive schemas, numeric and string constraints and complex array bounds, and it needs `additionalProperties: false` on every object. The TS reference got its schemas from `zodOutputFormat`, which strips the unsupported keywords and validates them client-side. The Rust crate has no SDK (ADR-R3-1), so it needs its own equivalent. The contract types also have to match the TS shape the web reads.

## Schemas

- **One envelope for every section.** `ReportSectionEnvelope { section, overview, weaknesses, openings, climbPlan, areas }` serves all five section calls, so the output format and the cached prefix are byte-identical. Every part is present-but-nullable: the key must be on the wire, and it is `null` when it is not the section asked for. The weekly review (`WeeklyReviewOutput`) and the change note (`WeaknessChangeOutput { text, citations }`) have their own schemas. Ids: `report-section`, `weekly-review` and `weakness-change`, all version 1, resolved by `schema_by_id` (batch custom ids carry both).
- **Sanitized schemars output.** `api_schema::<T>()` generates draft 2020-12 with `inline_subschemas = true` (no `$defs` and no `$ref`). It applies R2's `contract_transform`, the single mechanism for required plus nullable (every property is required, and an `Option` stays `T | null`), so the crate has no per-field schema wrappers. Then `sanitize` drops `$schema`, `title`, `format`, `minLength`, `maxLength`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `pattern`, `minItems`, `maxItems` and `default`. It rewrites `type: [X, "null"]` as `anyOf: [{…X}, {"type":"null"}]` and sets `additionalProperties: false` on every object. It also drops `type` beside `enum`, because the enum already fixes the type and the zod export has no `type` there. The rewrite descends only through schema positions (`properties`, `items`, the unions and `$defs`), so a property named like a keyword (`title`) survives.
- **Bounds are checked client-side.** Each `OutputSchema.check` first deserialises strictly with `serde_path_to_error`. Shape errors give one `ParseIssue` with a dotted path and a code (`invalid_type`, `unknown_field`, `missing_field` or `invalid_value`), and never the model's text. The check then enforces what the API cannot: every item text is 1 to `ITEM_TEXT_MAX_CHARS = 2000` code points (`too_small` / `too_big`), with at most 4 strengths, at most 3 focuses, 1 to 3 drills per focus, at most 3 weekly outcomes and at most 2 weekly exemplars. A failure is `ParseError::SchemaMismatch` with those issues.
- **The guard is ref-free and strict.** `assert_api_subset` walks every node. It allows only `type` (one basic type), `properties`, `required` (exactly the property names), `additionalProperties` (only `false`, and required on objects), `items`, `enum`, `const`, `anyOf` and `description`. That is narrower than what the API accepts, because `$ref`, `allOf` and string formats are excluded too, so a new contract cannot drift towards the edge of the subset. Tests run the guard on all three schemas and compare the envelope's shape with the TS export (`fixtures/ai/ts-export/report-section-envelope.schema.json`) using R2's shape walker.
- **Deviation from the TS export.** The SDK keeps `minItems: 1` on `drills`, which the API accepts. We drop every array bound and check `drills` client-side instead, so the shape test removes that one keyword from the TS side before comparing.

## Computed focuses and drills

Carried over from ADR-AI-7:

- Deterministic code computes or catalogues the focuses (which weaknesses to work on, at most 3), the opening verdicts (keep, fix, learn, replace) and the drill types. The model does not choose them.
- The drill catalogue `drills_for(weakness_id, variant)` (`output/drills.rs`, a port of `drills.ts`, improvement-engine §6.4) lists the allowed `DrillType`s for each weakness. It has 18 snake_case values. Unknown ids give an empty list. `W3` with variant `deep` gives `long_calculation` only. Every id except `W12` adds `own_game_analysis`.
- Claude only narrates, and for each selected focus it picks 1 to 3 drill types from the allowed list. The validator (Task 8) rejects anything else with `focus_not_selected`, `drill_not_allowed` or `verdict_mismatch`.
- Doses (how many drills, how often) belong to Milestone 6.

## Consequences

- The model cannot promote a weakness that was not selected, override a verdict or invent a drill. Changing the catalogue is a code change with tests.
- Adding a field to an output type changes its API schema automatically. The guard and the shape test fail if the new field leaves the subset or drifts from the TS contract.
- Bounds live in Rust code, not in the schema, so the model may emit an over-long text or array. That output is a schema mismatch and goes through the usual repair-once-then-drop path.
