/** Anthropic API error types; Rust's `classify_status` (T9) maps these bodies unchanged. */
export type ApiErrorType =
  | "invalid_request_error"
  | "authentication_error"
  | "permission_error"
  | "not_found_error"
  | "rate_limit_error"
  | "api_error"
  | "overloaded_error"
  /** Not an Anthropic type: another turn of the same conversation is running (409). */
  | "turn_in_progress";

export type ErrorBody = { type: "error"; error: { type: ApiErrorType; message: string } };

/** An error that becomes an HTTP status plus an Anthropic error body. Messages are fixed, never echoed. */
export class SidecarError extends Error {
  constructor(
    readonly status: number,
    readonly type: ApiErrorType,
    message: string,
    readonly retryAfter?: string,
  ) {
    super(message);
    this.name = "SidecarError";
  }

  body(): ErrorBody {
    return errorBody(this.type, this.message);
  }
}

export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ConfigError";
  }
}

export class Overloaded extends SidecarError {
  constructor() {
    super(529, "overloaded_error", "the sidecar is at capacity", "5");
    this.name = "Overloaded";
  }
}

/** The client went away while its request waited in the queue. Nothing is written back. */
export class ClientGone extends Error {
  constructor() {
    super("client disconnected");
    this.name = "ClientGone";
  }
}

export class SchemaUnsupported extends SidecarError {
  constructor(readonly path: string) {
    super(400, "invalid_request_error", `unsupported tool input schema at ${path}`);
    this.name = "SchemaUnsupported";
  }
}

export function errorBody(type: ApiErrorType, message: string): ErrorBody {
  return { type: "error", error: { type, message } };
}

export function typeForStatus(status: number): ApiErrorType {
  switch (status) {
    case 400:
      return "invalid_request_error";
    case 401:
      return "authentication_error";
    case 403:
      return "permission_error";
    case 404:
      return "not_found_error";
    case 429:
      return "rate_limit_error";
    case 529:
      return "overloaded_error";
    default:
      return status >= 500 ? "api_error" : "invalid_request_error";
  }
}

/** The upstream API failed with `status`. The message is generic: the SDK text may hold secrets. */
export function upstreamError(status: number, retryAfter?: string): SidecarError {
  return new SidecarError(
    status,
    typeForStatus(status),
    `upstream API error ${status}`,
    retryAfter,
  );
}

export const timeoutError = () => new SidecarError(504, "api_error", "the call timed out");
export const turnInProgress = () =>
  new SidecarError(409, "turn_in_progress", "another turn of this conversation is running");
export const shuttingDown = () =>
  new SidecarError(503, "api_error", "the sidecar is shutting down");
export const internalError = () => new SidecarError(500, "api_error", "the sidecar call failed");
export const noCredentials = () =>
  new SidecarError(401, "authentication_error", "the sidecar has no model credentials");
