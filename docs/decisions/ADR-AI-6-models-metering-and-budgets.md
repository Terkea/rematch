# ADR-AI-6: Models, metering and budgets

Status: accepted (placeholder number; Task I1 renumbers)

## Context

AI coaching calls Claude for report sections, short explanations, weekly reviews and chat. Cost must be predictable per user, and all calls go through the `AiClient` port (`packages/ai/src/client`) so tests never touch the network.

## Decision

### Model per feature

| Feature | Model | Effort |
|---|---|---|
| Report sections | `claude-sonnet-5-5` | `medium` |
| Chat | `claude-sonnet-5-5` | `low` |
| Short explanations (change notes) | `claude-haiku-5-5` | `low` |
| Weekly review | `claude-haiku-5-5` | `low` |

Exact IDs, no date suffix. `temperature`, `top_p`, `top_k`, `budget_tokens`, forced `tool_choice` and prefill are never sent (400 on these models); thinking is left unset (adaptive) and depth is controlled by `output_config.effort`. JSON comes from `output_config.format` built with `zodOutputFormat`.

The adapter sends the format through `messages.create` and validates with `schema.safeParse` itself rather than `messages.parse`: `parse` throws on a refusal or a truncated body, which would discard the message and its usage. A non-usable result is returned as `parsed: null` with `stopReason`.

### Why the weekly review is on Haiku

It summarises already-validated computed facts into a few sentences, runs for every active user every week (the highest call volume), and has no open-ended reasoning. Haiku 5.5 is 20x cheaper on input than Sonnet 5.5 at the same prompt size, and the validator (not the model) enforces correctness.

### Pricing (USD per MTok, equal to micro-USD per token)

Source: the Claude API pricing page (https://platform.claude.com/docs/en/about-claude/pricing), cached 2026-10-06; re-checked 2026-11-21 against the current `claude-api` model table (input/output unchanged).

| Model | Input | Output | Cache write (5 min) | Cache read | Long-prompt tier |
|---|---|---|---|---|---|
| sonnet-5-5 | 2.00 | 10.00 | 2.50 | 0.20 | none |
| haiku-5-5 | 0.10 | 0.50 | 0.125 | 0.01 | prompt above 100,000 tokens: 0.50 / 2.50 / 0.625 / 0.05 |

The long-prompt threshold counts input + cache write + cache read tokens. The Batch API halves the cost. Cache write and read rates are taken from the plan table (the skill lists only input and output); `MODEL_PRICING` carries a dated source comment. Costs are rounded to whole micro-USD.

### Budgets and the hard stop

`AI_MONTHLY_BUDGET_MICRO_USD = { free: 200_000, pro: 2_000_000 }` (USD 0.20 and USD 2.00 per month). `checkBudget(spent, plan)` returns `{ ok: false, reason: "budget" }` once spend reaches the budget. Callers check before each call and stop (hard stop, no overdraft); the report then renders computed-only. Usage from every response is metered with `costMicroUsd` after the call.

### Refusals and fallbacks

A refusal (`stop_reason: "refusal"`), a `max_tokens` truncation or an unparsable body is a failed attempt (`parsed: null`), handled by the caller's retry-then-drop policy. Server-side `fallbacks` is not used: it is beta-only and rejected on the Batches API. Revisit after the I2 live smoke.

### Errors and secrets

SDK errors are mapped by class/status (connection, 5xx, 529, 429 to `AiUnavailableError`; others to `AiRequestError`), never by message text. Error messages are built from the status only and never carry headers, the response body or the key; stray `sk-ant-` tokens are redacted from unknown errors. A circuit breaker (`withBreaker`) counts only `AiUnavailableError` as a failure and surfaces an open circuit as `AiUnavailableError("circuit_open")`.

## Consequences

Predictable per-user spend, no network in tests, and a single place to change models or prices. Prices must be re-checked when models change.
