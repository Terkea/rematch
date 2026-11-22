# ADR-R3-6. Claude calls go through a Claude Agent SDK sidecar

Date: 2026-11-21. Status: accepted (owner direction). Amends ADR-R3-1.

## Context

The owner asked that the AI layer use the same integration as their other project: the Claude Agent SDK (`@anthropic-ai/claude-agent-sdk`), which runs Claude Code under the hood. R3 had planned a hand-written reqwest client for the Messages API (Tasks 9–12). Task 9 (wire types, errors, redaction, config-driven cost, output parsing) is merged; Task 10 (SSE) was paused at tests only (WIP b36215f on r3/c).

Facts checked against the official docs on 2026-11-21 (code.claude.com/docs/en/agent-sdk/*):
- Auth: `ANTHROPIC_API_KEY`, Bedrock, Vertex, Foundry, or a subscription `CLAUDE_CODE_OAUTH_TOKEN`. Verbatim: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK. Use the API key authentication methods described in the Quickstart instead."
- Structured output: `options.outputFormat = { type: "json_schema", schema }` (draft-07); result in `structured_output`; failure subtype `error_max_structured_output_retries`.
- Prompt caching is automatic; TTL via `ENABLE_PROMPT_CACHING_1H=1`.
- No Message Batches support.
- One `query()` = one `claude` subprocess; docs suggest sizing about 1 GiB RAM per concurrent session.
- `total_cost_usd` is a client-side estimate, not billing; `modelUsage` gives per-model token counts.

## Decision

1. **A small Node service, `services/ai-sidecar`, owns every interactive Claude call** (report sections, weekly reviews, weakness-change notes, the coach chat). It wraps `query()` with: `maxTurns` small, no built-in tools (chat tools are exposed as in-process MCP tools that call back into Rust), `permissionMode: "dontAsk"`, `settingSources: []`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, an isolated `cwd` and `CLAUDE_CONFIG_DIR`, a custom `systemPrompt`, `outputFormat` for JSON sections, and pinned full model ids.
2. **Rust stays in charge.** Rust builds the prompts, owns budgets, the circuit breaker, metering and the fact-checker (validator), and talks to the sidecar over a local HTTP/JSON + server-sent-events contract. The sidecar holds no business rules. *Amended (Amendment 1, Task 10 fix round 1):* its only database access is the chat session store, which uses one table (`ai_sdk_session_entries`) through a restricted role (`ai_sidecar`, with SELECT/INSERT/DELETE on that table only). The sidecar reads no other data.
3. **Batches stay on the Messages API.** The monthly and weekly regenerations (Tasks 16, 20) keep the 50% batch discount through a minimal Rust batch client built on the merged Task 9 wire types. The planned streaming SSE client (Task 10) is dropped; streaming comes from the SDK.
4. **Cost is ours, not the SDK's estimate.** Metering computes cost from `modelUsage` token counts with the validated `PricingTable` (Task 9) and stores the SDK estimate only for comparison.
5. **Auth.** Production uses `ANTHROPIC_API_KEY` (or a cloud provider). The sidecar refuses to start in production with only a subscription token. Local development may use the owner's `CLAUDE_CODE_OAUTH_TOKEN`; the docs don't explicitly cover a developer testing their own product with a personal subscription, so this stays an open owner item.
6. **Supply chain.** The SDK is pinned exactly and passes the 7-day npm age check; its native `claude` binary comes from optional dependencies, so installs must not omit them.

## Consequences

- R3 lane C is re-planned: Task 10 becomes the sidecar and its contract; Task 11 becomes the Rust sidecar client plus the minimal batch client; Task 12 (breaker, fake client) is unchanged in purpose. Tasks 13, 18, 20, 23, 26 and 27 get hand-offs.
- Each concurrent call costs a subprocess, so the sidecar has a concurrency cap and a queue; capacity planning uses measured RSS.
- If the subprocess model proves too heavy at scale, the Rust contract lets us swap the sidecar for a direct Messages client without touching callers.

## Contract and isolation (Task 10)

This section was recorded with the implementation in `services/ai-sidecar` (SDK `0.3.325`). Task 11 codes against this contract, so change the two together.

**Endpoints.** All bodies are JSON with camelCase keys. Every route except `/health` needs `authorization: Bearer ${AI_SIDECAR_TOKEN}` and answers 401 without it.
- `POST /v1/query` makes one stateless structured call. The request carries `model, effort, maxTokens, timeoutMs, maxTurns, systemPrompt, prompt[] (text blocks), outputFormat?`. The reply is 200 for any SDK result message: `{subtype, stopReason, structuredOutput?, text, numTurns, durationMs, modelUsage{model: {inputTokens, outputTokens, cacheReadInputTokens, cacheCreationInputTokens}}, sdkCostUsd}`. `subtype` passes through: `success`, `error_max_turns`, `error_max_structured_output_retries` or `error_during_execution`. When the result carries `api_error_status`, the reply instead has that HTTP status and an Anthropic error body.
- `POST /v1/chat` runs one chat turn over SSE. The request is the query body without `outputFormat`, plus:
  - `tools[] {name, description, inputSchema}`;
  - `toolCallback {url, token}`, required when `tools` is not empty (Task 11: a turn without tools, such as the "answer now" follow-up, omits it and mounts no coach server);
  - `conversationId` (a uuid);
  - `resume` (a boolean);
  - `rebuildContext`, required when `resume` is true (otherwise 400).

  The stream sends these events:
  - `delta {text, messageId?}`, for top-level `text_delta` only;
  - `retract {messageIds, reason}`, naming streamed messages that a refusal or a superseding message withdrew (`done.text` is authoritative);
  - `tool_use {id, name, input, messageId?}` (a later retract naming `messageId` withdraws the call), where `name` is the SDK's MCP name `mcp__coach__<name>` (Rust strips the prefix);
  - exactly one terminal event, either `done` (the query result plus `session: "new" | "resumed" | "rebuilt"` and `sessionModelUsage`) or `error` (the error body plus `status`);
  - a `: ping` comment every 15 s.

  A client disconnect aborts the query.
- `GET /health` needs no token, makes no model call and returns no secret. Its body is `{ok, auth, inFlight, queued, maxConcurrency, sdkVersion, mirrorErrors}`.
- The tool callback is `POST {toolCallback.url}` with `Bearer {toolCallback.token}` and the body `{name, input}`. Rust answers `{content, isError}`. A non-2xx reply, a malformed body, a network error or a timeout becomes the tool result `{"available":false,"reason":"tool unavailable"}` with `isError: true`. The model never sees a status or a URL.

**Error bodies.** Every error uses the Anthropic shape `{"type":"error","error":{"type","message"}}`, so Rust's `classify_status` (T9) maps it unchanged. Statuses map to types like this:
- 400 → `invalid_request_error`;
- 401 → `authentication_error`;
- 403 → `permission_error`;
- 429 → `rate_limit_error`, with `retry-after` from the SDK's last `api_retry` delay;
- 529 → `overloaded_error`;
- any other status ≥ 500 → `api_error`.

A timeout is 504 `api_error`, and a call aborted by shutdown (SIGTERM) is 503 `api_error`. A second concurrent turn of one conversation is 409 with the non-Anthropic type `turn_in_progress`; Rust should treat it as "busy, retry after the running turn". Messages are fixed strings and never echo SDK text, because that text may contain a key.

**Isolation.** `buildOptions` is the single place where the options are set:
- `model` comes from an allowlist (`claude-sonnet-5-5`, `claude-haiku-5-5`) and is never an alias.
- `maxTurns` is capped at 10.
- `systemPrompt` is the custom string from the request.
- `tools: []`, `strictMcpConfig: true`, `permissionMode: "dontAsk"` and `settingSources: []`.
- `verbatimPrompts: true`, so user text is never expanded as `@path` or dispatched as a slash command.
- `cwd` is an empty work dir.
- `agents`, `hooks`, `fallbackModel`, `thinking` and `maxThinkingTokens` are never set.
- `/v1/query` sets `persistSession: false` and `outputFormat`, and uses no session.
- `/v1/chat` sets `mcpServers: {coach}`, `allowedTools` (exactly the `mcp__coach__<name>` names) and `includePartialMessages: true`. It also sets `sessionId` on the first turn or `resume` on later turns, together with `sessionStore`.
- Chat never sets `persistSession: false` or `enableFileCheckpointing`, because the SDK rejects both when a store is present.

**Env allowlist.** The child's `env` replaces the parent environment. It holds only these variables:
- `PATH`;
- `HOME` and `CLAUDE_CONFIG_DIR`, both set to the config dir;
- `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`;
- `CLAUDE_CODE_MAX_OUTPUT_TOKENS`;
- `CLAUDE_AGENT_SDK_CLIENT_APP=chess-improver-sidecar/0.1`;
- `CLAUDE_CODE_PROJECT_DIR_NAME=chess-improver-coach`, which keeps the session store's `projectKey` stable across replicas and paths;
- the one credential that `auth` chose: the API key, the OAuth token (development only), or, for `cloud`, the `CLAUDE_CODE_USE_*` flag and the provider variables.

`AI_SIDECAR_TOKEN`, `AI_SESSION_DATABASE_URL`, `DATABASE_URL` and every other variable are left out.

**Chat sessions (owner direction, controller ruling).** Chat uses the SDK's native sessions, and the session id is the Postgres conversation id.
- **Turns.** The first turn runs with `sessionId`. Later turns run with `resume` and send only the new user message.
- **Storage.** A Postgres `SessionStore` (`PgSessionStore`) mirrors the transcripts into one table, `ai_sdk_session_entries`. Its columns are `project_key text`, `session_id uuid`, `subpath text NOT NULL DEFAULT ''`, `seq bigint`, `entry jsonb` and `created_at timestamptz`. The primary key is `(project_key, session_id, subpath, seq)`.
- **Access.** The sidecar connects as the dedicated role `ai_sidecar`, which has only SELECT, INSERT and DELETE on that table. Task 17 owns the migration, the role and the GRANT; the sidecar's tests use a fixture copy.
- **Entries.** They are opaque JSON. SQL reads only the documented top-level `uuid`, as the idempotency key for retried batches. Appends are serialized per transcript with an advisory lock.
- **Conformance.** The adapter passes a port of the SDK's conformance suite, which the npm package does not ship.
- **Fallback.** Sometimes the transcript is unknown: neither a store row nor a local file exists. Sometimes the resume fails before any output: an `error_during_execution` result, or a throw with no result. In either case the sidecar clears that session's rows and retries once as a new session. The retry's prompt is Rust's `rebuildContext` followed by the new message.
- **Mirror errors.** Each `system/mirror_error` increments the `mirrorErrors` warning metric and logs a warning without detail.
- **Local disk.** The CLI's own disk (`/var/lib/ai-sidecar`) is a tmpfs scratch area, with no named volume.
- **Deletion.** Rust deletes the `ai_sdk_session_entries` rows for the conversation id in the same transaction as the conversation (Tasks 17 and 25). The sidecar has no delete endpoint.
- **JSON calls.** One-shot JSON calls stay stateless.

**JSON Schema to zod.** Rust owns the tool schemas as JSON Schema, but `tool()` needs a zod raw shape. The converter accepts only the T22 subset:
- a top-level `object` with `properties`, `required`, `additionalProperties: false` and an optional `description`;
- the property types `string` (optionally with `enum`), `integer` (optionally with `minimum`/`maximum`), `number`, `boolean`, and `array` of `string`.

Properties that are not required become `.optional()`. Anything else throws `SchemaUnsupported(path)`, which `/v1/chat` returns as a 400 before anything is queued or spawned.

**Tool schemas have no nullable form (Task 22).** The converter has no `anyOf` and no `null`, so the chat tools (`chat::tools`) do not use the required-and-nullable form of the output schemas. An optional property is left out of `required` and becomes `.optional()`. The Rust executor (`chat::execute`) validates every field again and treats a missing optional field and `null` the same way. An unexpected key is rejected with the fixed message `invalid input: unexpected field`, which never names the key.

**Timeout ordering.** The timeouts nest from shortest to longest:
1. T23's engine wait is 45 s.
2. The callback fetch times out at 55 s.
3. The MCP per-server tool-call limit (`createSdkMcpServer({ timeout })`) is 60 s.

Separately, each request's `timeoutMs` aborts the whole query.

**Limiter.** At most `maxConcurrency` 4 calls run at once, each in its own subprocess of about 1 GiB. Further requests wait in a FIFO queue of up to 32 for at most 30 s. When the queue is full or the wait runs out, the reply is 529 `overloaded_error` with `retry-after: 5`. A client that disconnects while queued leaves the queue. Compose caps the container with `mem_limit: 5g`, and capacity planning uses measured RSS.

**Production guard.** `AI_SIDECAR_ENV=production` or `NODE_ENV=production` turns the guard on. With `auth` of `oauth` or `none`, the sidecar exits with code 78 and the message "refusing to start: production needs ANTHROPIC_API_KEY or a cloud provider; a subscription token is not allowed (ADR-R3-6)". Production also requires `AI_SESSION_DATABASE_URL`. In development `oauth` is allowed, and `none` starts, reports `auth:"none"` and answers every call with 401 `authentication_error`.

**Licence.** `@anthropic-ai/claude-agent-sdk` is "SEE LICENSE IN README.md": Anthropic's commercial terms, not open source. Its peers are permissive: `@modelcontextprotocol/sdk` 1.32.2 (MIT), `@anthropic-ai/sdk` 1.8.0 (Apache-2.0) and `zod` 4.6.7 (MIT). So is `pg` 8.23.1 (MIT). All are pinned exactly and pass the 7-day age check. The platform `claude` binaries come from the SDK's `optionalDependencies`, so installs must keep optional dependencies.

## Amendment 1 (2026-11-21): chat uses SDK sessions stored in Postgres

Owner direction: the coach chat uses the SDK's native sessions, as in the owner's reference project. The plain Messages API is stateless, so the history has to be stored either way.

- **Sessions.** A new conversation runs `sessionId = conversation uuid`, and each later turn runs `resume = conversation uuid` with only the new user message. Claude sees its own earlier replies verbatim, and caching across turns is automatic (5-minute TTL; `ENABLE_PROMPT_CACHING_1H=1` for longer).
- **Storage.** We use the documented pluggable `options.sessionStore` (agent-sdk/session-storage). The sidecar's Postgres adapter writes opaque entries to one table, `ai_sdk_session_entries`, through a dedicated role granted access to that table only. The CLI's local transcript disk is scratch (tmpfs). This supersedes "the sidecar has no database access" for this single table.
- **System of record.** Postgres conversations and messages (Tasks 17 and 25) remain the record for display, titles, rollover summaries and deletion. If a resume fails, the sidecar retries once as a new session seeded with Rust's rebuilt context (summary plus recent turns).
- **Deletion and retention.** The SDK never deletes from a store. Rust deletes a conversation's `ai_sdk_session_entries` rows in the same transaction as the conversation itself, so "delete conversation" and "delete all" leave nothing behind.
- **One-shot calls.** Report sections and other JSON one-shot calls stay stateless.
- **Unverified items, tested in Task 10:** the version that added `SessionStore`, the result shape for a missing session on resume, and the `deleteSession` signature.

## Notes from the Task 10 review (fix round 1)

- **Verbatim prompts.** `buildOptions` sets `verbatimPrompts: true`, which requires Claude Code 2.1.248 or later; the bundled CLI is 2.1.325. Without it, the CLI expands user text such as `@/proc/self/environ` as a file mention and dispatches `/compact`-style text as a slash command. With it, prompts are delivered as written, and the CLI's turn-start attachment pass (nested CLAUDE.md files, skill listings) is skipped too. That suits a sidecar with an empty cwd and no settings.
- **Cumulative versus per-turn usage, and metering (controller ruling, fix round 2).** The SDK's result `modelUsage` is cumulative for the session: on a resumed turn it includes every earlier turn, because the transcript carries the saved totals. The sidecar reports two fields.
  - `sessionModelUsage` is the SDK's running total.
  - `modelUsage` is the turn's own usage per model. On a resumed turn it is summed from the stream's `message_start` and `message_delta` usage. On a new or rebuilt turn it is the SDK's figure, which is per-turn there and also counts internal calls. When a failed resume attempt preceded a rebuild, that attempt's streamed usage is added.

  **Rust meters as follows.** For `/v1/query`, and for new or rebuilt chat turns, it meters `modelUsage`. For a resumed turn, it meters `sessionModelUsage(now) − sessionModelUsage(previous turn)` per model; Rust keeps each turn's `sessionModelUsage` for this. If any difference is negative (for example, after a rebuild reset the totals), it falls back to `modelUsage`. The difference also captures internal calls that the stream does not show.

  `/v1/query` is stateless, so its `modelUsage` is per call. Model keys pass through unchanged, including a refusal-fallback model outside the allowlist, and Rust prices unknown keys conservatively.
- **Local transcripts are scratch.** Before every new or rebuilt attempt, the sidecar removes `configDir/projects/<projectKey>/<id>.jsonl` and `<id>/`, because the CLI rejects a `sessionId` whose file exists. With a store configured, it also removes them after each turn. The compose service has a read-only root and sized tmpfs mounts for `/var/lib/ai-sidecar` (512 MiB) and `/tmp` (256 MiB).
- **Retractions (SDK-faithful, fix round 2).** Deltas carry the `messageId` of their API message. A `retract {messageIds, reason}` event names only messages the sidecar actually streamed and has not already retracted. It is triggered in three ways:
  - When a streamed message ends with `stop_reason: "refusal"`, its retract (`reason: "refusal"`) is sent at once, before the fallback model's replacement text.
  - An assistant message's `supersedes` triggers a retract (`reason: "superseded"`).
  - A `model_refusal_fallback`'s `retracted_message_uuids` triggers a retract (`reason: "model_refusal_fallback"`).

  The uuids in the last two are mapped to message ids, and unknown or already-retracted uuids are a no-op. `done.text` is authoritative for the turn's final text.
- **One turn per conversation.** Two overlapping turns of one conversation would delete each other's transcripts and branch the stored session. The sidecar holds an in-process lock per conversation id. When a store is configured, it also holds a Postgres session-level advisory lock, `pg_try_advisory_lock(2026112110, hashtext(conversation_id))`, on a dedicated pool connection for the whole turn, so replicas exclude each other. The two-int key keeps it apart from the store's single-bigint append locks. The lock is taken once a limiter slot is free, so a queued request holds no connection. A second turn gets 409 with error type `turn_in_progress`, before any header. On a `new` turn whose id already has store rows (a retried first turn), those rows are deleted first.
- **Store writes.** `append` is `INSERT … ON CONFLICT DO NOTHING` against Task 17's partial unique index `(project_key, session_id, subpath, (entry->>'uuid')) WHERE entry ? 'uuid'`, so retried batches and in-batch repeats are no-ops. The `ai_sidecar` role is created NOLOGIN by the migration. Operations grant `LOGIN PASSWORD` out of band; in dev compose that is `docker/postgres/ai-sidecar-role.sh` (profile `ai`).

## Notes from Task 11

- **Optional callback.** `toolCallback` is optional when `tools` is empty, and such a turn mounts no `coach` MCP server. A callback that is sent is still validated.
- **Turn-lock hygiene (final Task 10 review).** `lockLost` counts only while the lock is held. A no-op `error` listener stays on the lock connection after release (once per pooled client, so listeners do not pile up). On the not-acquired path a failing `RESET` destroys the connection and still reports "held elsewhere" (409), instead of throwing, which would have let the turn run with no lock.
- **Test schema.** The sidecar's store tests read the `ai_sdk_session_entries` table, indexes and role block from Task 17's real migration (`db/migrations/20261121030000_ai_coaching.sql`) at test time; the copied fixture is gone.

## Interactive engine checks (Task 24)

The chat's `analyse_position` tool reaches Stockfish only through `ai::chat::interactive::DbInteractiveEngine`, one per turn (account, assistant message, turn start).

- **Protocol.** `sf19-n1m-mpv3-interactive-v1`: Stockfish 19, 1,000,000 nodes, MultiPV 3, a 30 s timeout. It is deliberately not R1's `sf19-n300k-v1`. The searches are deeper, run on positions the player made up, and are requested on demand, so they are not comparable with the pinned per-game analysis.
- **The separate store.** Results live only in `interactive_evals` (`source = 'interactive'`), as `lines = [{multipv, score (White's point of view), pv (UCI), san, depth, nodes}]`. `InteractiveAnalyseHandler` never writes `position_evals`, and KPI, stats, profile and report code never read `interactive_evals`. A "what if" line from a hypothetical position must not move a player's accuracy, weakness rates or cached evals, and a protocol mix in `position_evals` would break the R1 cache key. A test asserts that the `position_evals` count is unchanged.
- **Queue.** Each check is an `interactive-analyse` job on R1's Postgres queue: priority 1, `user_key` = account, dedupe `ieval:<id>`, one attempt. It runs on the shared engine pool, so the claim ordering, leases and clock-step rules of ADR 0032 apply unchanged. The request polls its row every 100 ms for up to the caller's wait. When the wait passes first it returns `Pending`, and Task 23 then calls `wait` or `abandon`.
- **Quotas.** Free plans get 3 checks a week and Pro plans 20 a day (`entitlements::engine_quotas`, UTC windows). A charged row is one use. The reservation is atomic: `db::interactive::reserve_eval` takes `pg_advisory_xact_lock(30002, hashtext(account))`, counts charged rows per window in new statements under READ COMMITTED, and inserts the pending row only if every quota has room, all in one transaction. A test fires N+1 requests in parallel against N remaining checks and accepts exactly N, and it fails when the lock is removed. A request over the quota writes nothing and returns `Limit { window, resets_at }`. Task 22's cap of 2 checks per turn is applied before any of this, so a third call gets `per_turn` without touching the quota.
- **Reuse.** A `done` row of the same account, FEN (normalised after the moves) and protocol within 24 h is copied into a new `done` row with `charged = false`, linked to the new message. No engine call is made, and this works even when the quota is used up.
- **Refunds (`charged = false`).**
  - An engine failure other than a closed pool marks the row `failed`.
  - A dead-lettered job (`on_dead`) marks the row `failed`.
  - `abandon` refunds the check (account-scoped).
  - A lost job is failed by `ai-reconcile` step 6.
  - A closed pool (shutdown) releases the job after 5 s without failing the row.
- **Cancellation (T23 m6).** A drop guard closes this gap. If a `request` or `wait` future is dropped while it polls (the turn was cancelled), the guard spawns the refund. A check whose result nobody saw is never charged. If its job still finishes, the result stays reusable for free.
- **Scoping.** Eval ids are 32 lowercase hex (`Uuid::simple`). `wait` and `abandon` only see the engine's own account, so a foreign id is `Unavailable` or a no-op.
