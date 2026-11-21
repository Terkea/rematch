/**
 * chess.com public profile lookup (read-only Published-Data API, no auth). Used to check that a
 * username exists and to read its `location` field for the verification code (ADR-WEB-5).
 *
 * - The User-Agent is the project's anonymous one (ADR 0043), the same string the worker sends; no
 *   email address ever goes into a request header.
 * - Requests are serial: one in flight at a time per process, as chess.com asks of API clients.
 * - A 429 is retried once after a short backoff (Retry-After, capped); a second 429 gives up.
 */

/** ADR 0043. Must equal the worker's `USER_AGENT` (apps/worker/src/http/platform-client.ts; tested). */
export const CHESSCOM_USER_AGENT = "chess-improver/0.1 (+https://github.com/chess-improver)";
/** Largest profile body read; chess.com profiles are a few KB. */
export const MAX_BODY_BYTES = 256 * 1024;
export const DEFAULT_TIMEOUT_MS = 5000;
/** Longest wait for a 429's Retry-After before the one retry; the request is interactive. */
export const MAX_BACKOFF_MS = 2000;

/**
 * The API base must be https, except in development and tests (the e2e fakes and the unit-test
 * mock server are plain http). Throws otherwise.
 */
export function assertChesscomBaseUrl(baseUrl: string, nodeEnv = process.env.NODE_ENV): void {
  const protocol = new URL(baseUrl).protocol;
  if (protocol === "https:") return;
  if (protocol === "http:" && (nodeEnv === "test" || nodeEnv === "development")) return;
  throw new Error("CHESSCOM_API_BASE_URL must be https");
}

/** Reads the body as JSON, giving up beyond `MAX_BODY_BYTES`. */
async function readJson(res: Response): Promise<unknown> {
  const declared = Number(res.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > MAX_BODY_BYTES) {
    await res.body?.cancel().catch(() => {});
    throw new ChesscomUnavailableError("profile body too large");
  }
  if (!res.body) return null;
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      await reader.cancel().catch(() => {});
      throw new ChesscomUnavailableError("profile body too large");
    }
    chunks.push(value);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    return null;
  }
}

export class ChesscomUnavailableError extends Error {
  constructor(reason: string) {
    super(`chess.com is unavailable: ${reason}`);
    this.name = "ChesscomUnavailableError";
  }
}

export type ChesscomProfile = {
  /** Lower-case, as chess.com stores it. */
  username: string;
  /** The username's own capitalisation, from the profile URL when present. */
  displayName: string;
  location: string | null;
};

export type ChesscomDeps = {
  fetch: typeof fetch;
  baseUrl: string;
  timeoutMs?: number;
  /** Backoff before the single 429 retry (tests pass a no-op). */
  sleep?: (ms: number) => Promise<void>;
};

const USERNAME_SHAPE = /^[a-z0-9_-]{3,25}$/;

const realSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

// Promise-chain queue: one chess.com request at a time in this process.
let tail: Promise<unknown> = Promise.resolve();
function serial<T>(work: () => Promise<T>): Promise<T> {
  const run = tail.then(work, work);
  tail = run.catch(() => {});
  return run;
}

function retryAfterMs(res: Response): number {
  const seconds = Number(res.headers.get("retry-after"));
  const ms = Number.isFinite(seconds) && seconds > 0 ? seconds * 1000 : 1000;
  return Math.min(ms, MAX_BACKOFF_MS);
}

async function getOnce(url: string, deps: ChesscomDeps): Promise<Response> {
  const timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  try {
    return await deps.fetch(url, {
      headers: { "User-Agent": CHESSCOM_USER_AGENT, Accept: "application/json" },
      signal: AbortSignal.timeout(timeoutMs),
      redirect: "error",
    });
  } catch (error) {
    const name = (error as { name?: string } | null)?.name;
    if (name === "TimeoutError" || name === "AbortError") {
      throw new ChesscomUnavailableError(`no response within ${timeoutMs} ms`);
    }
    throw new ChesscomUnavailableError("network error");
  }
}

function displayNameFrom(url: unknown, username: string): string {
  if (typeof url !== "string") return username;
  const last = url.split("/").filter(Boolean).at(-1) ?? "";
  return last.toLowerCase() === username ? last : username;
}

/**
 * The public profile for `username`, or null when chess.com has no such player (404, or 410 for a
 * closed account). Throws `ChesscomUnavailableError` on 403, 429, 5xx, a timeout or a bad body.
 */
export function fetchChesscomProfile(
  username: string,
  deps: ChesscomDeps,
): Promise<ChesscomProfile | null> {
  const url = `${deps.baseUrl.replace(/\/+$/, "")}/pub/player/${encodeURIComponent(username.toLowerCase())}`;
  const sleep = deps.sleep ?? realSleep;
  return serial(async () => {
    let res = await getOnce(url, deps);
    if (res.status === 429) {
      await res.body?.cancel().catch(() => {});
      await sleep(retryAfterMs(res));
      res = await getOnce(url, deps);
    }
    if (res.status === 404 || res.status === 410) {
      await res.body?.cancel().catch(() => {});
      return null;
    }
    if (!res.ok) {
      await res.body?.cancel().catch(() => {});
      throw new ChesscomUnavailableError(`HTTP ${res.status}`);
    }
    const body = (await readJson(res)) as Record<string, unknown> | null;
    const name = typeof body?.username === "string" ? body.username.toLowerCase() : "";
    // The profile must be the one asked for, and a name we would accept ourselves.
    if (!body || name !== username.toLowerCase() || !USERNAME_SHAPE.test(name)) {
      throw new ChesscomUnavailableError("unexpected profile body");
    }
    return {
      username: name,
      displayName: displayNameFrom(body.url, name),
      location: typeof body.location === "string" ? body.location : null,
    };
  });
}
