# ADR-R3-1. Rust Anthropic client and the `ai` crate's dependencies

Date: 2026-11-21
Status: Accepted (crate table, R3 Task 1). Task 11 completes the client part. Placeholder number; Task 26 renumbers it. Builds on 0030 and 0031.

## Context

R3 ports the TS AI coaching layer (TS M3, `packages/ai`) to one Rust crate, `ai`. It runs in parallel lanes, so as in R1 and R2 one bootstrap task declares every dependency, module and shared type, and commits `Cargo.lock`. Later R3 tasks never edit a `Cargo.toml` dependency list or the lockfile.

## Decision

The Anthropic client is our own reqwest client (Messages, streaming SSE and Batches over the workspace `reqwest`/`rustls`). No Anthropic SDK crate is added. Task 11 records the client design.

### New crates

Each was checked on crates.io on 2026-11-21: newest stable release, published at least 7 days earlier, licence on the allowlist. All are pinned exactly in `[workspace.dependencies]`.

| crate | version | published | licence | why |
|---|---|---|---|---|
| serde_path_to_error | =0.1.20 | 2025-09-15 | MIT OR Apache-2.0 | dotted error paths for parse issues |
| unicode-normalization | =0.1.26 | 2026-10-16 | (MIT OR Apache-2.0) AND Unicode-3.0 | NFKC for the leak detector (it was already in the lockfile as a transitive dependency at this version) |

Already in the workspace table (R2, ADR-R2-1), reused at the same pins: schemars =1.2.3 (MIT; features `derive`, `uuid1`) and sha2 =0.11.1 (MIT OR Apache-2.0) for prompt and input hashes, plus hex =0.4.3 for their lowercase-hex form.

From R1's table, unchanged: reqwest, rustls, tokio, tokio-util, futures, bytes, serde, serde_json, thiserror, anyhow, tracing, uuid, time, sqlx, regex, rand, cozy-chess, wiremock (dev). The plan's `chrono` is not used (R1 amendment, ADR 0031): timestamps are `time::OffsetDateTime`.

### Crate wiring

- `ai` depends on `chess-core`, `common`, `db`, `queue`, `events`, `engine` (for `ANALYSIS_PROTOCOL` only) and `coaching-input` (for the Task 26 reconciliation with R2's contract type), and dev-depends on `testkit`, `wiremock` and tokio `test-util`.
- `worker` depends on `ai` now, so Task 26 can wire it without editing a manifest.
- No crate may enable `serde_json/preserve_order`: `canonical_json` and the path index rely on sorted keys. CI's `rust` job checks `cargo tree -e features -i serde_json | grep -c preserve_order` is `0`.

### Shared types and fixtures

- `ai::input::types` is a field-for-field serde mirror of the zod `CoachingInput`. Nullable fields are `Option`s whose key must be present, and their JSON schema stays required and nullable (`Nullable<T>`, because `#[schemars(required)]` would drop the `null`). `tests/input_shape.rs` compares its schemars schema with the zod export.
- `backend/fixtures/ai/input/` holds byte-for-byte copies of the three M3 fixtures. `backend/fixtures/ai/ts-export/` holds `z.toJSONSchema(CoachingInput)` and `zodOutputFormat(ReportSectionEnvelope).schema` from m3/base@4ff8220, each wrapped as `{"source":"m3/base@4ff8220","schema":…}`. They were exported in `node:22.23.5-slim` with `pnpm install --frozen-lockfile` and Node type stripping on a throwaway copy of the m3 checkout. Only the JSON is committed. The CoachingInput export equals R2's `fixtures/coaching-input/ts-schema.json`.

## Consequences

- Lanes add behaviour inside the declared modules only. A lane that needs a new crate stops and reports it.
- `ai::input::types` and R2's `coaching-input` crate duplicate one contract until Task 26 reconciles them.
