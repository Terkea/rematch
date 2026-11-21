import Anthropic from "@anthropic-ai/sdk";

export type AiUnavailableReason =
  | "connection"
  | "server"
  | "overloaded"
  | "rate_limited"
  | "circuit_open"
  | "no_api_key";

export class AiUnavailableError extends Error {
  constructor(
    readonly reason: AiUnavailableReason,
    message?: string,
  ) {
    super(message ?? `AI unavailable: ${reason}`);
    this.name = "AiUnavailableError";
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

/**
 * Maps SDK errors to our typed errors by class and status (never by message). Messages are
 * built here from status only: the SDK message embeds the response body, and the original
 * error (which holds headers) is never attached as `cause`.
 */
export function classifyAiError(e: unknown): Error {
  if (e instanceof AiUnavailableError || e instanceof AiRequestError) return e;
  if (e instanceof AiBudgetExceededError) return e;
  if (e instanceof Anthropic.APIConnectionError) return new AiUnavailableError("connection");
  if (e instanceof Anthropic.APIError) {
    const status = e.status;
    if (e instanceof Anthropic.OverloadedError || status === 529) {
      return new AiUnavailableError("overloaded");
    }
    if (e instanceof Anthropic.RateLimitError) return new AiUnavailableError("rate_limited");
    if (e instanceof Anthropic.InternalServerError || (status !== undefined && status >= 500)) {
      return new AiUnavailableError("server");
    }
    return new AiRequestError(`AI request rejected (HTTP ${status ?? "unknown"})`, status ?? 0);
  }
  if (e instanceof Error) {
    const out = new Error(redact(e.message));
    out.name = e.name;
    return out;
  }
  return new Error(redact(String(e)));
}
