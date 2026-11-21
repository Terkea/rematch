# ADR-R3-5. Models, budgets and degradation

Date: 2026-11-21
Status: Accepted ("Models and pricing" and "Budgets", R3 Task 9). Task 18 appends "Degradation". Placeholder number; Task 26 renumbers it. Ports ADR-AI-6 (TS M3) to the Rust `ai` crate and builds on ADR-R3-1.

## Context

The coaching layer calls Claude for report sections, change notes, weekly reviews and chat. Cost must be predictable per account, every call goes through the `AiClient` port, and tests never touch the network. The client is our own reqwest client (ADR-R3-1), so the wire shapes, pricing and the error taxonomy live in `ai::client::{wire, cost, errors, parse, custom_id}`.

## Models and pricing

### Model and effort per use (ADR-AI-6)

| Use | Model | Effort |
|---|---|---|
| Report sections | `claude-sonnet-5-5` | `medium` |
| Chat | `claude-sonnet-5-5` | `low` |
| Change notes | `claude-haiku-5-5` | `low` |
| Weekly review | `claude-haiku-5-5` | `low` |

- Exact ids, with no date suffix. `PricingTable::get` matches ids exactly, so a dated snapshot is an unknown model.
- Effort is always sent in `output_config.effort`, because Haiku 5.5 defaults to `medium`.
- `thinking` is never sent. Both models run adaptive thinking by default, and depth is set only by effort.
- These are never serialised:
  - `temperature`, `top_p`, `top_k` (each one is a 400 on API version `2026-10-21`);
  - `budget_tokens` (removed on both models);
  - a forced `tool_choice` (`any`/`tool` return a 400). `ToolChoice` has only `auto` and `none`;
  - `fallbacks` (beta, and rejected on the Batches API);
  - assistant prefill.
- `wire::MessagesRequest` is a plain struct, so its field order is fixed. A test checks that none of these keys appear anywhere in a serialised request.
- JSON comes only from `output_config.format` (`{"type":"json_schema","schema":…}`). `parse_model_output` checks the result itself, in this order: refusal, `max_tokens`, no schema, no text, invalid JSON, schema check. A refusal or a truncation keeps the message and its usage.
- Chat turns stream and use strict tools with `eager_input_streaming: true`. Tools are sorted by name, and a top-level `cache_control` is sent. With no tools, neither `tools` nor `tool_choice` is sent.

### Why the weekly review is on Haiku

The weekly review summarises facts that were already computed and validated into a few sentences. It runs for every active account every week, which makes it the highest call volume, and it needs no open-ended reasoning. Haiku 5.5 input costs a twentieth of Sonnet 5.5 input at the same prompt size. The validator, not the model, enforces correctness.

### Pricing (USD per MTok, equal to micro-USD per token)

Source: https://platform.claude.com/docs/en/about-claude/pricing, read on 2026-11-21 (ruling Q5: re-check live at T9). We also compared it with the `claude-api` skill's model table (cached 2026-10-06). Prices only; no API call was made.

**Prices come from validated config (fix round 1).**
- `cost_micro_usd(&PricingTable, model, usage, batch)` takes a `PricingTable`. The binary loads it once at startup with `PricingTable::from_env()`:
  - when `AI_PRICING_FILE` names a JSON file, that file is used;
  - otherwise the compiled-in dated seed `MODEL_PRICING` is used.
- The format is documented in `ai::client::cost`. The seed file is `backend/crates/ai/config/pricing.json`. Its header fields `source` (the URL above), `as_of` (`2026-11-21`) and `unit` record where and when the prices were read, and a test keeps the file equal to the const.
- The table is validated before use. An invalid table is a startup error, not a silent default:
  - every `AiModel::ALL` id must be priced;
  - every rate must be finite and ≥ 0, with at most 3 decimals, so the integer metering stays exact;
  - `long_prompt.above` must be > 0;
  - unknown keys are rejected.
- A price change is a config change (plus a dated header) rather than a code change. Changing the seed still needs a code change and this ADR.

| Model | Input | Output | Cache write (5 min) | Cache read | Long-prompt tier |
|---|---|---|---|---|---|
| `claude-sonnet-5-5` | 2.00 | 10.00 | 2.50 | **0.10** | none (1M context at standard rates) |
| `claude-haiku-5-5` | 0.10 | 0.50 | 0.125 | 0.01 | prompt above 100,000 tokens: 0.50 / 2.50 / 0.625 / 0.05 |

- **Change from the plan and ADR-AI-6.** The plan had Sonnet 5.5 cache reads at 0.20. The live page prices Sonnet 5.5 (and Opus 5.5) cache hits at 0.05x input, which is 0.10. We use the live value. The ported `cost.test.ts` expectations change from 14000 / 7000 to 13000 / 6500. The TS `MODEL_PRICING` still says 0.20 and should be corrected when the TS layer is retired or touched.
- The long-prompt threshold counts input + cache write + cache read tokens and is strictly "above". Each request is priced on its own.
- The Batch API halves the cost. The cache multipliers stack with it.
- `cost_micro_usd` uses exact integer arithmetic: rates are converted to thousandths of a micro-USD, so every rate in the table is exact. It rounds half up to whole micro-USD.
- **Only the 5-minute TTL is priced.** We send `cache_control: {"type":"ephemeral"}` only. If a 1-hour TTL (`"ttl":"1h"`) is ever adopted, metering must read `usage.cache_creation.ephemeral_1h_input_tokens` separately and price it at the 2x input rate (Sonnet 5.5: 4.00; Haiku 5.5: 0.20, or 1.00 above the long-prompt threshold). `cache_creation_input_tokens` mixes both TTLs.
- The minimum cacheable prefix is 512 tokens on both models (prompt-caching docs, re-checked 2026-11-21). A request has at most 4 breakpoints.
- **Unknown response models.** `cost_micro_usd` returns `AiError::UnknownModel` for an id it does not price. That id is redacted, and the error never contains a key. The metering site prices a response whose `model` is not in the table at the **requested** model's rates and logs a `warn` with both ids. A surprise model id then never loses a usage row and never stops a job.

### Errors and secrets

- Errors are classified by status and API `error.type`, never by message:
  - 529 or `overloaded_error` gives `Unavailable(Overloaded)`;
  - 429 gives `Unavailable(RateLimited)` with `retry-after` (seconds or HTTP date);
  - 5xx gives `Unavailable(Server)`;
  - 401/403 give `Auth`;
  - any other status gives `Request { status, api_error_type }`.
  - 402 (`billing_error`), 413 and every other 4xx are non-retryable `Request` errors.
- Transport errors are classified the same way:
  - reqwest builder and redirect errors are configuration faults. They give the fatal `Request { status: 0, api_error_type: "client_builder_error" | "client_redirect_error" }`, never a retryable error;
  - a body that does not decode gives `Response` with fixed text, because serde messages can echo content;
  - everything else gives `Unavailable(Connection)`.
- **Open for the retry task (`client/retry.rs`): 408 and 409.** The Anthropic SDKs retry 408 (request timeout) and 409 (conflict), as well as 429, 5xx and connection errors. Here they are currently plain `Request` errors, so they are not retried. The retry task decides on parity. The likely choice is to retry them with the same backoff as `Server`, while keeping the taxonomy unchanged.
- Errored batch items are classified by type:
  - `overloaded_error`/`api_error` are retryable;
  - `invalid_request_error`/`authentication_error`/`permission_error` are fatal;
  - anything else is unknown.
- `redact` replaces `sk-ant-[A-Za-z0-9_-]+`. Anything taken from a response (the error type, a model id) is redacted before it goes into an error.
- The User-Agent is `chess-improver-ci (+https://github.com/chess-improver)`. No personal contact data is ever sent.

## Budgets

- **Per account.** Spend since the start of the calendar month (UTC) is hard-stopped at `AI_MONTHLY_BUDGET_MICRO_USD = Budgets { free: 200_000, pro: 2_000_000 }` (USD 0.20 and USD 2.00). The plan comes from `entitlements::Plan`.
- **Global.** There is an optional monthly hard stop across all accounts (`AI_GLOBAL_MONTHLY_BUDGET_MICRO_USD`, from configuration). It works the same way.
- **Checked before every call.** `check_budget(spent, plan, &budgets)` returns `Err(AiError::Budget)` once spend reaches the budget (`>=`). It runs before every model call, both per account and globally. This is a hard stop with no overdraft. The overshoot is bounded by one call, because spend is only known after the response.
- **Metered after every call.** Usage from every response, including a refusal, a truncation or a failed parse, is metered into `ai_usage` with `cost_micro_usd`. Batch usage is metered at half price.
- When the budget is exhausted, reports render computed-only and chat says it is unavailable. Task 18 records the full degradation rules.

## Consequences

- Spend per account is predictable, tests make no network calls, and there is one place to change models or prices.
- Prices must be re-checked against the live pricing page whenever a model changes. The Sonnet 5.5 cache-read correction shows that the skill table can lag behind that page.
