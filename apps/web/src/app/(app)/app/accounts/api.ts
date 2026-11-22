import type { LinkedAccountView } from "../../../../server/accounts/list";

// The browser side of the account routes (Task 19). Mutations send JSON, which the routes'
// CSRF check requires (ADR-WEB-5); the browser adds the same-origin `Origin` itself.

/** Task 23 replaces this with `queryKeys.accounts()`; Task 27 swaps it in. */
export const accountsKey = ["accounts"] as const;

export type StartedLink = {
  linkId: string;
  username: string;
  code: string | null;
  verified: boolean;
};
export type VerifyResult = { status: "verified" | "pending" | "expired"; nextCheckAt: string };

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly retryAt?: string,
  ) {
    super(code);
    this.name = "ApiError";
  }
}

async function call<T>(path: string, init?: { method: string; body?: unknown }): Promise<T> {
  const res = await fetch(path, {
    method: init?.method ?? "GET",
    credentials: "same-origin",
    headers: init ? { "content-type": "application/json" } : { accept: "application/json" },
    ...(init?.body === undefined ? {} : { body: JSON.stringify(init.body) }),
  });
  if (res.status === 204) return undefined as T;
  const data = (await res.json().catch(() => null)) as
    | (T & { error?: string; retryAt?: string })
    | null;
  if (!res.ok) throw new ApiError(res.status, data?.error ?? "unknown", data?.retryAt);
  return data as T;
}

export const fetchAccounts = () => call<LinkedAccountView[]>("/api/accounts");

export const startChesscomLink = (username: string) =>
  call<StartedLink>("/api/accounts/chesscom", { method: "POST", body: { username } });

export const verifyChesscomLink = (linkId: string) =>
  call<VerifyResult>(`/api/accounts/chesscom/${linkId}/verify`, { method: "POST", body: {} });

export const removeAccountLink = (linkId: string) =>
  call<void>(`/api/accounts/${linkId}`, { method: "DELETE", body: {} });

const clock = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });

/** What went wrong and what to do, for the linking routes' error codes. */
export function linkErrorMessage(error: unknown, username?: string): string {
  if (!(error instanceof ApiError))
    return "That didn't go through. Check your connection and try again.";
  switch (error.code) {
    case "invalid_username":
      return "Enter a chess.com username: 3 to 25 letters, numbers, hyphens or underscores.";
    case "not_found":
      return username ? `chess.com has no player called ${username}.` : "That account is gone.";
    case "limit_reached":
      return "You can link up to 3 chess.com accounts. Remove one to add another.";
    case "rate_limited":
      return error.retryAt
        ? `chess.com checks are paused for a moment. You can check again at ${clock.format(new Date(error.retryAt))}.`
        : "chess.com checks are paused for a moment. You can check again in a few minutes.";
    case "chesscom_unavailable":
      return "chess.com isn't answering. Try again in a minute.";
    case "unauthenticated":
      return "You've been signed out. Sign in again to carry on.";
    default:
      return "That didn't go through. Try again.";
  }
}
