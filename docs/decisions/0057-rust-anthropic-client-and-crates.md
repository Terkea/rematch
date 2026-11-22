# 0057. Rust Anthropic client and the `ai` crate's dependencies

Date: 2026-11-21
Status: Accepted (crate table, R3 Task 1; client design, R3 Task 11). Renumbered by R3 Task 26 (it was the R3 placeholder record). Builds on 0030 and 0031. Amends ADR 0011. Amended by ADR 0062.

## Context

R3 ports the TS AI coaching layer (TS M3, `packages/ai`) to one Rust crate, `ai`. It runs in parallel lanes, so as in R1 and R2 one bootstrap task declares every dependency, module and shared type, and commits `Cargo.lock`. Later R3 tasks never edit a `Cargo.toml` dependency list or the lockfile.

## Decision

~~The Anthropic client is our own reqwest client (Messages, streaming SSE and Batches).~~ Superseded by ADR 0062: interactive calls go through the Agent SDK sidecar, and only batches use our own reqwest client. No Anthropic SDK crate is added. The client design is recorded below (Task 11).

### New crates

Each was checked on crates.io on 2026-11-21: newest stable release, published at least 7 days earlier, licence on the allowlist. All are pinned exactly in `[workspace.dependencies]`.

| crate | version | published | licence | why |
|---|---|---|---|---|
| serde_path_to_error | =0.1.20 | 2025-09-15 | MIT OR Apache-2.0 | dotted error paths for parse issues |
| unicode-normalization | =0.1.26 | 2026-10-16 | (MIT OR Apache-2.0) AND Unicode-3.0 | NFKC for the leak detector (it was already in the lockfile as a transitive dependency at this version) |

Already in the workspace table (R2, ADR-R2-1), reused at the same pins: schemars =1.2.3 (MIT; features `derive`, `uuid1`) and sha2 =0.11.1 (MIT OR Apache-2.0) for prompt and input hashes, plus hex =0.4.3 for their lowercase-hex form.

From R1's table, unchanged: reqwest, rustls, tokio, tokio-util, futures, bytes, serde, serde_json, thiserror, anyhow, tracing, uuid, time, sqlx, regex, rand, cozy-chess, wiremock (dev). The plan's `chrono` is not used (R1 amendment, ADR 0031): timestamps are `time::OffsetDateTime`.

### Crate wiring

- `ai` depends on `chess-core`, `common`, `db`, `queue`, `events`, `engine` (for `ANALYSIS_PROTOCOL` only) and `coaching-input` (the contract type), and dev-depends on `testkit`, `wiremock` and tokio `test-util`.
- `worker` depends on `ai` now, so Task 26 can wire it without editing a manifest.
- No crate may enable `serde_json/preserve_order`: `canonical_json` and the path index rely on sorted keys. CI's `rust` job checks that `cargo tree -e features -i serde_json | grep -c preserve_order` is `0`, failing closed if `cargo tree` fails; an `ai` unit test asserts sorted map keys.

### Shared types and fixtures

- `ai::input::types` re-exports R2's `coaching-input` crate (ADR-R2-2), so there is one contract type (fix round 1). Required + nullable JSON schemas use one mechanism, R2's `contract_transform` (every property required, `Option` stays `T | null`), which is re-exported for Task 3's output schemas. Serde-side, nullable keys must be present (`coaching_input::nullable`, now public). `tests/input_shape.rs` runs R2's schema walker (`coaching-input/tests/support/shape.rs`, shared via `#[path]`) against the zod export.
- `backend/fixtures/ai/input/` holds byte-for-byte copies of the three M3 fixtures. `backend/fixtures/ai/ts-export/` holds `z.toJSONSchema(CoachingInput)` and `zodOutputFormat(ReportSectionEnvelope).schema` from m3/base@4ff8220, each wrapped as `{"source":"m3/base@4ff8220","schema":…}`. Provenance (full commit, zod options, sha256) is in `fixtures/ai/ts-export/SOURCE`. They were exported in `node:22.23.5-slim` with `pnpm install --frozen-lockfile` and Node type stripping on a throwaway copy of the m3 checkout. Only the JSON is committed. The CoachingInput export equals R2's `fixtures/coaching-input/ts-schema.json`.

### Client design (Task 11)

This amends ADR 0011 (Claude API with structured outputs): the call path changes, the structured-output contract and the claim validator do not.

- **Interactive calls go through the sidecar** (`client::sidecar`, ADR 0062). `structured` is `POST /v1/query` with `outputFormat` and `maxTurns: 3`; `stream_turn` is `POST /v1/chat` over SSE and is one whole agentic turn. `AiRuntime` implements `AiClient`: sidecar for interactive calls, `BatchesClient` for batches. Bodies are camelCase serde types (`client::sidecar_wire`). The bearer is `SidecarToken` (`AI_SIDECAR_TOKEN`); the URL is `AI_SIDECAR_URL`.
- **Rust flattens the conversation** (`client::flatten`). The SDK takes user messages only, so the system blocks are joined with a blank line and earlier messages become tagged text blocks (`<previous_user_message>`, `<previous_assistant_output>` with assistant text only, `<instruction>` for a mid-conversation system message), followed by the last user message. `cache_control` is dropped: caching on the sidecar path is automatic.
- **Our schema check is authoritative.** The order is refusal, `max_tokens`, the SDK's `error_max_structured_output_retries` (schema mismatch, no issues), a missing output (no text), then `schema.check` on `structuredOutput` in Rust. The SDK's own validation never replaces ours. `content` carries the serialised output as one text block, so repair prompts can replay it.
- **Usage and metering.** Results carry `model_usage` per model (the sum is `usage`) and the SDK's `sdk_cost_usd`, kept only for comparison. Chat turns also carry `session_model_usage` and the `session` path; `port::metered_turn_usage` applies the ADR 0062 rule (a resumed turn meters the difference of session totals, falling back to `modelUsage` when negative).
- **Streaming events.** `delta.messageId` becomes `TurnEvent::Message` before the first delta of each API message, `retract` becomes `TurnEvent::Retract`, `tool_use` events are collected into `TurnResult.tool_uses`, `done` gives `Done` (its text is authoritative) and `error` is classified by status and type. A body that ends before a terminal event, or is idle longer than `stream_idle_timeout` (60 s, which also bounds the wait for headers while the sidecar queues), is `Unavailable(Connection)`. Undecodable data is `Response` with fixed text.
- **The tool endpoint** (`client::tool_server`) is axum on `AI_TOOL_BIND` (default `0.0.0.0:7311`), reachable from the sidecar at `AI_TOOL_PUBLIC_URL`. Each turn registers its `ToolHandler` under a per-turn token (32 random bytes, hex); dropping the registration unregisters it, so turns never cross. The client drops it as soon as the terminal event is queued, and every token also expires after a TTL of `chat_timeout + TIMEOUT_GRACE + stream_idle_timeout`, the longest a turn can run on our side (fix rounds 1 and 2). 401 without a token, 404 for an unknown or expired one, a 64 KiB body limit, and a 50 s per-call timeout that answers `isError` with `{"available":false,"reason":"engine busy"}` (T23's 45 s engine wait < 50 s < the sidecar's 55 s callback fetch). Tokens are looked up by SHA-256, compared in constant time and never logged.
- **The batch client** (`client::batches`) is our own reqwest client at `anthropic-version: 2026-10-21`, because the SDK has no batch support and there is no official Rust SDK. `submit`, `status` and `results` use `ANTHROPIC_API_KEY` (`ApiKey`); without it `batches_enabled()` is false and the batch calls give `Unavailable(NoApiKey)`. A dev OAuth token never reaches Rust. Results are always read from `{base}/v1/messages/batches/{id}/results`; the batch's `results_url` is never followed, so a forged host never receives the key. Batch ids are checked against `[A-Za-z0-9_-]` before they go into a path.
- **Retries** (`client::retry`). Retryable: a connection failure, 408, 409, 429, 5xx and 529, overridden by `x-should-retry`. A `retry-after` at or under the cap (30 s for batches) is slept; over the cap the call gives up at once and surfaces `retry_after`. Otherwise `base * 2^attempt` plus 0 to 250 ms of jitter. `status` retries up to 2 times (the results stream does not retry). `submit` creates a paid, non-idempotent batch, so it retries only when no batch can have been created: a refused connection, 429 or 529. A 5xx, a timeout after sending, or any other failure is returned for the reconciler (Task 21) to resolve (fix round 1). The sidecar client retries at most once, only before the first response byte, and only on a refused connection or a 529, because the sidecar may already be running the call after any other failure. A 409 `turn_in_progress` is "busy" and never retried within the turn.
- **Limits.** A single SSE line or event, and a single results JSONL line, is capped at 4 MiB. Beyond it the chat stream ends with `Response`, and the results stream yields one `Response` error, skips that line and goes on. Both scanners search each byte for a newline only once.
- **User-Agent (ADR 0043).** Every request the `ai` crate sends carries the one shared `wire::USER_AGENT`, `chess-improver/0.1 (+https://github.com/chess-improver)`, with no email address.
- **Secrets.** `SidecarToken` and `ApiKey` redact in `Debug` and `Display` and have no `Serialize`; the tool callback token is redacted in `Debug`. Error types read from bodies are kept only when they look like an API type (`[a-z_]{1,64}`), and `redact` (the `sk-ant-` pattern, which also covers OAuth `sk-ant-oat…` tokens) runs on every error message. Logs are `debug` only: method, path, status, attempt, duration and `request-id`.
- **New crate:** `axum =0.8.9` (MIT, released 2026-04-14), `default-features = false` with `tokio`, `http1` and `json`, added by the coordinator. It reuses the hyper 1.x already in the lockfile; the lockfile gains only `axum`, `axum-core`, `matchit` and `mime`. `cargo deny` and `scripts/check-crate-age.sh` pass.

## Consequences

- Lanes add behaviour inside the declared modules only. A lane that needs a new crate stops and reports it.
