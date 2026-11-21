import type {
  ModelUsage,
  SDKMessage,
  SDKResultMessage,
  SDKUserMessage,
} from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import { internalError, type SidecarError, timeoutError, upstreamError } from "./errors.js";
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
  modelUsage: Record<string, TokenCounts>;
  sdkCostUsd: number;
};

export type QueryDeps = {
  query?: QueryFn;
  limiter: Limiter;
  logger: Logger;
  /** Aborts the call when the HTTP client goes away. */
  signal?: AbortSignal;
};

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

/** An AbortController that fires on the request timeout or when `parent` (the client) aborts. */
export function callController(timeoutMs: number, parent?: AbortSignal) {
  const ac = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    ac.abort();
  }, timeoutMs);
  const onParent = () => ac.abort();
  parent?.addEventListener("abort", onParent, { once: true });
  return {
    ac,
    timedOut: () => timedOut,
    dispose: () => {
      clearTimeout(timer);
      parent?.removeEventListener("abort", onParent);
    },
  };
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
      const ctl = callController(req.timeoutMs, deps.signal);
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
        if (!final) {
          const e = ctl.timedOut() ? timeoutError() : internalError();
          deps.logger.warn({ ...log, status: e.status, durationMs: Date.now() - started });
          throw e;
        }
      } finally {
        ctl.dispose();
      }
      if (!final) {
        const e = ctl.timedOut() ? timeoutError() : internalError();
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
      const out = mapResult(final);
      deps.logger.info({ ...log, status: 200, ...usageFields(out) });
      return out;
    },
    deps.signal ? { signal: deps.signal } : {},
  );
}
