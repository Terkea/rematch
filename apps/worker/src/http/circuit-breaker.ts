import type { PlatformClient, RequestInitLite } from "./platform-client.js";

export class CircuitOpenError extends Error {
  constructor(readonly retryInMs: number) {
    super(`Circuit open: platform calls paused for another ${retryInMs} ms`);
    this.name = "CircuitOpenError";
  }
}

export type CircuitState = "closed" | "open" | "half-open";

export interface CircuitBreakerOptions {
  /** Consecutive failures that open the circuit (default 5). */
  failureThreshold?: number;
  /** How long the circuit stays open before one probe is allowed (default 60 s). */
  cooldownMs?: number;
  /** Which errors count as an outage (default: all). A 404 is the caller's problem, not one. */
  isFailure?: (err: unknown) => boolean;
  now?: () => number;
}

export interface CircuitBreaker {
  exec<T>(fn: () => Promise<T>): Promise<T>;
  state(): CircuitState;
}

/**
 * Closed: calls pass. After `failureThreshold` consecutive failures it opens and rejects with
 * CircuitOpenError without calling `fn`. After `cooldownMs` it is half-open: a single probe goes
 * through; success closes it, failure re-opens it for another cooldown.
 */
export function createCircuitBreaker(opts: CircuitBreakerOptions = {}): CircuitBreaker {
  const threshold = opts.failureThreshold ?? 5;
  const cooldownMs = opts.cooldownMs ?? 60_000;
  const isFailure = opts.isFailure ?? (() => true);
  const now = opts.now ?? Date.now;
  let failures = 0;
  let openedAt: number | null = null;
  let probing = false;

  const state = (): CircuitState => {
    if (openedAt === null) return "closed";
    return now() - openedAt >= cooldownMs ? "half-open" : "open";
  };

  return {
    state,
    async exec<T>(fn: () => Promise<T>): Promise<T> {
      const s = state();
      if (s === "open" || (s === "half-open" && probing)) {
        throw new CircuitOpenError(Math.max(0, (openedAt ?? 0) + cooldownMs - now()));
      }
      const probe = s === "half-open";
      if (probe) probing = true;
      try {
        const result = await fn();
        failures = 0;
        openedAt = null;
        return result;
      } catch (err) {
        if (isFailure(err)) {
          failures++;
          if (probe || failures >= threshold) openedAt = now();
        } else if (probe) {
          // The platform answered: it is up.
          failures = 0;
          openedAt = null;
        }
        throw err;
      } finally {
        if (probe) probing = false;
      }
    },
  };
}

/** A PlatformClient whose every request (and every streamed chunk) goes through `breaker`. */
export function withCircuitBreaker(
  client: PlatformClient,
  breaker: CircuitBreaker,
): PlatformClient {
  return {
    getJson: <T>(url: string, init?: RequestInitLite) =>
      breaker.exec(() => client.getJson<T>(url, init)),
    async *getNdjson<T>(url: string, init?: RequestInitLite): AsyncIterable<T> {
      let it: AsyncIterator<T> | undefined;
      try {
        for (;;) {
          const res = await breaker.exec(async () => {
            it ??= client.getNdjson<T>(url, init)[Symbol.asyncIterator]();
            return it.next();
          });
          if (res.done) return;
          yield res.value;
        }
      } finally {
        await it?.return?.();
      }
    },
  };
}
