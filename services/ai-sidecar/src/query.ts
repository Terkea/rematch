import type {
  ModelUsage,
  SDKMessage,
  SDKResultMessage,
  SDKUserMessage,
} from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import {
  internalError,
  type SidecarError,
  shuttingDown,
  timeoutError,
  upstreamError,
} from "./errors.js";
import type { Limiter } from "./limiter.js";
import type { LogFields, Logger } from "./log.js";
import { buildOptions, type QueryRequest, type TextBlock } from "./options.js";

export type QueryFn = typeof sdk.query;

export type TokenCounts = {
  inputTokens: number;
  outputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
};

export type SidecarResult = {
  subtype: SDKResultMessage["subtype"];
  stopReason: string | null;
  structuredOutput?: unknown;
  text: string;
  numTurns: number;
  durationMs: number;
  /** This call's (this turn's) usage per model. Rust meters this field. */
  modelUsage: Record<string, TokenCounts>;
  /**
   * Chat only: the SDK's running session total, which on a resumed turn includes every earlier
   * turn. For comparison and diagnostics; never meter it.
   */
  sessionModelUsage?: Record<string, TokenCounts>;
  sdkCostUsd: number;
};

export type QueryDeps = {
  query?: QueryFn;
  limiter: Limiter;
  logger: Logger;
  /** Aborts the call when the HTTP client goes away. */
  signal?: AbortSignal;
  /** Aborts the call when the process shuts down (503). */
  shutdown?: AbortSignal;
};

const ZERO = (): TokenCounts => ({
  inputTokens: 0,
  outputTokens: 0,
  cacheReadInputTokens: 0,
  cacheCreationInputTokens: 0,
});

/**
 * Per-turn usage summed from the stream (`includePartialMessages`): each `message_start` opens an
 * API message with its model and input-side usage, and each `message_delta` carries the message's
 * cumulative usage so far, so the latest value per message wins. Messages are summed per model.
 */
export class StreamUsage {
  private readonly messages = new Map<string, { model: string; u: TokenCounts }>();
  private current: string | undefined;
  private seq = 0;

  observe(m: SDKMessage): void {
    if (m.type !== "stream_event") return;
    const ev = m.event as {
      type?: string;
      message?: { id?: string; model?: string; usage?: Record<string, unknown> };
      usage?: Record<string, unknown>;
    };
    if (ev.type === "message_start") {
      this.seq += 1;
      const key = `${ev.message?.id ?? "anon"}#${this.seq}`;
      const entry = { model: ev.message?.model ?? "unknown", u: ZERO() };
      apply(entry.u, ev.message?.usage);
      this.messages.set(key, entry);
      this.current = key;
    } else if (ev.type === "message_delta" && this.current) {
      const entry = this.messages.get(this.current);
      if (entry) apply(entry.u, ev.usage);
    }
  }

  totals(): Record<string, TokenCounts> {
    const out: Record<string, TokenCounts> = {};
    for (const { model, u } of this.messages.values()) {
      const t = out[model] ?? ZERO();
      t.inputTokens += u.inputTokens;
      t.outputTokens += u.outputTokens;
      t.cacheReadInputTokens += u.cacheReadInputTokens;
      t.cacheCreationInputTokens += u.cacheCreationInputTokens;
      out[model] = t;
    }
    return out;
  }

  get empty(): boolean {
    return this.messages.size === 0;
  }
}

function apply(t: TokenCounts, u: Record<string, unknown> | undefined): void {
  if (!u) return;
  const n = (k: string) => (typeof u[k] === "number" ? (u[k] as number) : undefined);
  t.inputTokens = n("input_tokens") ?? t.inputTokens;
  t.outputTokens = n("output_tokens") ?? t.outputTokens;
  t.cacheReadInputTokens = n("cache_read_input_tokens") ?? t.cacheReadInputTokens;
  t.cacheCreationInputTokens = n("cache_creation_input_tokens") ?? t.cacheCreationInputTokens;
}

/** The request's text blocks as one streaming-input user message (the only way to send blocks). */
export async function* userPrompt(blocks: TextBlock[]): AsyncGenerator<SDKUserMessage> {
  yield {
    type: "user",
    message: { role: "user", content: blocks },
    parent_tool_use_id: null,
  } as SDKUserMessage;
}

export function mapResult(m: SDKResultMessage): SidecarResult {
  const modelUsage: Record<string, TokenCounts> = {};
  for (const [model, u] of Object.entries(m.modelUsage ?? {}) as Array<[string, ModelUsage]>) {
    modelUsage[model] = {
      inputTokens: u.inputTokens,
      outputTokens: u.outputTokens,
      cacheReadInputTokens: u.cacheReadInputTokens,
      cacheCreationInputTokens: u.cacheCreationInputTokens,
    };
  }
  const out: SidecarResult = {
    subtype: m.subtype,
    stopReason: m.stop_reason ?? null,
    text: m.subtype === "success" && typeof m.result === "string" ? m.result : "",
    numTurns: m.num_turns,
    durationMs: m.duration_ms,
    modelUsage,
    sdkCostUsd: m.total_cost_usd,
  };
  if (m.subtype === "success" && m.structured_output !== undefined) {
    out.structuredOutput = m.structured_output;
  }
  return out;
}

/** The upstream HTTP status carried by a result, if the API call itself failed. */
export function apiErrorStatus(m: SDKResultMessage): number | undefined {
  const s = (m as { api_error_status?: number | null }).api_error_status;
  return typeof s === "number" && s >= 400 ? s : undefined;
}

/** Tracks the latest `system/api_retry` delay so a 429 can forward `retry-after`. */
export class RetryAfterTracker {
  private delayMs: number | undefined;
  observe(m: SDKMessage): void {
    if (m.type === "system" && (m as { subtype?: string }).subtype === "api_retry") {
      const d = (m as { retry_delay_ms?: unknown }).retry_delay_ms;
      if (typeof d === "number" && d > 0) this.delayMs = d;
    }
  }
  header(status: number): string | undefined {
    if (status !== 429 || this.delayMs === undefined) return undefined;
    return String(Math.ceil(this.delayMs / 1000));
  }
}

export function usageFields(r: SidecarResult): Partial<LogFields> {
  const sum = {
    inputTokens: 0,
    outputTokens: 0,
    cacheReadInputTokens: 0,
    cacheCreationInputTokens: 0,
  };
  for (const u of Object.values(r.modelUsage)) {
    sum.inputTokens += u.inputTokens;
    sum.outputTokens += u.outputTokens;
    sum.cacheReadInputTokens += u.cacheReadInputTokens;
    sum.cacheCreationInputTokens += u.cacheCreationInputTokens;
  }
  return { subtype: r.subtype, numTurns: r.numTurns, durationMs: r.durationMs, ...sum };
}

export type AbortCause = "timeout" | "shutdown" | "client" | undefined;

/**
 * An AbortController for one call. It fires on the request timeout, on process shutdown, or when
 * the client goes away, and remembers which came first.
 */
export function callController(timeoutMs: number, client?: AbortSignal, shutdown?: AbortSignal) {
  const ac = new AbortController();
  let cause: AbortCause;
  const fire = (c: Exclude<AbortCause, undefined>) => {
    if (cause === undefined) cause = c;
    ac.abort();
  };
  const timer = setTimeout(() => fire("timeout"), timeoutMs);
  const onClient = () => fire("client");
  const onShutdown = () => fire("shutdown");
  if (shutdown?.aborted) fire("shutdown");
  else shutdown?.addEventListener("abort", onShutdown, { once: true });
  if (client?.aborted) fire("client");
  else client?.addEventListener("abort", onClient, { once: true });
  return {
    ac,
    cause: () => cause,
    timedOut: () => cause === "timeout",
    dispose: () => {
      clearTimeout(timer);
      client?.removeEventListener("abort", onClient);
      shutdown?.removeEventListener("abort", onShutdown);
    },
  };
}

/** The error for a call that ended without a usable result. */
export function abortError(cause: AbortCause): SidecarError {
  if (cause === "timeout") return timeoutError();
  if (cause === "shutdown") return shuttingDown();
  return internalError();
}

/** Runs one stateless structured call (`/v1/query`). */
export async function runQuery(
  req: QueryRequest,
  cfg: SidecarConfig,
  deps: QueryDeps,
): Promise<SidecarResult> {
  const query = deps.query ?? sdk.query;
  const log: LogFields = { msg: "call", route: "/v1/query", model: req.model };
  return deps.limiter.run(
    async () => {
      const started = Date.now();
      const ctl = callController(req.timeoutMs, deps.signal, deps.shutdown);
      const retry = new RetryAfterTracker();
      let final: SDKResultMessage | undefined;
      try {
        const options = buildOptions(req, cfg, { abortController: ctl.ac, partial: false });
        for await (const m of query({ prompt: userPrompt(req.prompt), options })) {
          retry.observe(m);
          if (m.type === "result") final = m;
        }
      } catch {
        // A single-shot query throws after an error result; the result is what counts.
      } finally {
        ctl.dispose();
      }
      if (!final) {
        const e = abortError(ctl.cause());
        deps.logger.warn({ ...log, status: e.status, durationMs: Date.now() - started });
        throw e;
      }
      const status = apiErrorStatus(final);
      if (status !== undefined) {
        const e: SidecarError = upstreamError(status, retry.header(status));
        deps.logger.warn({
          ...log,
          status,
          numTurns: final.num_turns,
          durationMs: final.duration_ms,
        });
        throw e;
      }
      // Stateless: the SDK's modelUsage covers exactly this one call.
      const out = mapResult(final);
      deps.logger.info({ ...log, status: 200, ...usageFields(out) });
      return out;
    },
    deps.signal ? { signal: deps.signal } : {},
  );
}
