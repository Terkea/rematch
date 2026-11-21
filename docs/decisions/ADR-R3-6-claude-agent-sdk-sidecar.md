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
2. **Rust stays in charge.** Rust builds the prompts, owns budgets, the circuit breaker, metering and the fact-checker (validator), and talks to the sidecar over a local HTTP/JSON + server-sent-events contract. The sidecar has no database access and holds no business rules.
3. **Batches stay on the Messages API.** The monthly and weekly regenerations (Tasks 16, 20) keep the 50% batch discount through a minimal Rust batch client built on the merged Task 9 wire types. The planned streaming SSE client (Task 10) is dropped; streaming comes from the SDK.
4. **Cost is ours, not the SDK's estimate.** Metering computes cost from `modelUsage` token counts with the validated `PricingTable` (Task 9) and stores the SDK estimate only for comparison.
5. **Auth.** Production uses `ANTHROPIC_API_KEY` (or a cloud provider). The sidecar refuses to start in production with only a subscription token. Local development may use the owner's `CLAUDE_CODE_OAUTH_TOKEN`; the docs don't explicitly cover a developer testing their own product with a personal subscription, so this stays an open owner item.
6. **Supply chain.** The SDK is pinned exactly and passes the 7-day npm age check; its native `claude` binary comes from optional dependencies, so installs must not omit them.

## Consequences

- R3 lane C is re-planned: Task 10 becomes the sidecar and its contract; Task 11 becomes the Rust sidecar client plus the minimal batch client; Task 12 (breaker, fake client) is unchanged in purpose. Tasks 13, 18, 20, 23, 26 and 27 get hand-offs.
- Each concurrent call costs a subprocess, so the sidecar has a concurrency cap and a queue; capacity planning uses measured RSS.
- If the subprocess model proves too heavy at scale, the Rust contract lets us swap the sidecar for a direct Messages client without touching callers.
