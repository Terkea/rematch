import Anthropic from "@anthropic-ai/sdk";
import type { AiErrorKind } from "./types.js";

export type AiUnavailableReason =
  | "connection"
  | "server"
  | "overloaded"
  | "rate_limited"
  | "circuit_open"
  | "no_api_key";

export class AiUnavailableError extends Error {
  readonly retryAfterMs: number | undefined;
  constructor(
    readonly reason: AiUnavailableReason,
    message?: string,
    opts?: { retryAfterMs?: number | undefined },
  ) {
    super(message ?? `AI unavailable: ${reason}`);
    this.name = "AiUnavailableError";
    this.retryAfterMs = opts?.retryAfterMs;
  }
}

export class AiRequestError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = "AiRequestError";
  }
}

/** 401/403: a configuration problem. The job layer should fail fast and alert, not retry. */
export class AiAuthError extends AiRequestError {
  constructor(status: number) {
    super(`AI credentials rejected (HTTP ${status})`, status);
    this.name = "AiAuthError";
  }
}

/** A response or stream that could not be decoded (not a transport failure). */
export class AiResponseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AiResponseError";
  }
}

export class AiBudgetExceededError extends Error {
  constructor(message = "AI budget exceeded") {
    super(message);
    this.name = "AiBudgetExceededError";
  }
}

const KEY_PATTERN = /sk-ant-[A-Za-z0-9_-]+/g;

/** Strip anything that looks like an API key. */
export function redact(text: string): string {
  return text.replace(KEY_PATTERN, "[redacted]");
}

/** Parses a `retry-after` header (seconds or HTTP date) into milliseconds. */
export function parseRetryAfterMs(
  value: string | null | undefined,
  now = Date.now(),
): number | undefined {
  if (!value) return undefined;
  const secs = Number(value);
  if (Number.isFinite(secs) && secs >= 0) return Math.round(secs * 1000);
  const at = Date.parse(value);
  return Number.isNaN(at) ? undefined : Math.max(0, at - now);
}

/** Taxonomy for the `error.type` of an errored batch item. */
export function classifyBatchErrorType(type: string | null | undefined): AiErrorKind {
  switch (type) {
    case "overloaded_error":
    case "api_error":
      return "retryable";
    case "invalid_request_error":
    case "authentication_error":
    case "permission_error":
      return "fatal";
    default:
      return "unknown";
  }
}

/**
 * Maps SDK errors to our typed errors by class and status (never by message). Messages are
 * built here from status only: the SDK message embeds the response body, and the original
 * error (which holds headers) is never attached as `cause`.
 */
export function classifyAiError(e: unknown): Error {
  if (e instanceof AiUnavailableError || e instanceof AiRequestError) return e;
  if (e instanceof AiBudgetExceededError || e instanceof AiResponseError) return e;
  if (e instanceof Anthropic.APIConnectionError) return new AiUnavailableError("connection");
  if (e instanceof Anthropic.APIError) {
    const status = e.status;
    if (e instanceof Anthropic.OverloadedError || status === 529) {
      return new AiUnavailableError("overloaded");
    }
    if (e instanceof Anthropic.RateLimitError) {
      const retryAfterMs = parseRetryAfterMs(e.headers?.get("retry-after"));
      return new AiUnavailableError("rate_limited", undefined, { retryAfterMs });
    }
    if (e instanceof Anthropic.InternalServerError || (status !== undefined && status >= 500)) {
      return new AiUnavailableError("server");
    }
    if (status === 401 || status === 403) return new AiAuthError(status);
    return new AiRequestError(`AI request rejected (HTTP ${status ?? "unknown"})`, status ?? 0);
  }
  if (e instanceof Error) {
    const out = new Error(redact(e.message));
    out.name = e.name;
    return out;
  }
  return new Error(redact(String(e)));
}

/**
 * Classifies a failure raised while reading a batch results stream: SDK errors keep the normal
 * taxonomy, SDK decode errors become AiResponseError, anything else (socket reset, abort) is a
 * connection failure. Messages are redacted.
 */
export function classifyStreamError(e: unknown): Error {
  const c = classifyAiError(e);
  if (c instanceof AiUnavailableError || c instanceof AiRequestError) return c;
  if (c instanceof AiResponseError) return c;
  if (e instanceof Anthropic.AnthropicError || e instanceof SyntaxError) {
    return new AiResponseError(c.message);
  }
  return new AiUnavailableError("connection", redact(`AI stream failed: ${c.message}`));
}
