import { AiUnavailableError } from "./errors.js";
import type { AiClient } from "./types.js";

export interface Breaker {
  exec<T>(fn: () => Promise<T>): Promise<T>;
}

/**
 * Wraps a client so calls run through a circuit breaker. Only AiUnavailableError counts as a
 * failure: other errors (bad request, budget) are carried out of the breaker as values so it
 * records a success, then rethrown. A rejection that did not come from `fn` means the breaker
 * itself refused the call, which becomes AiUnavailableError("circuit_open").
 */
export function withBreaker(ai: AiClient, breaker: Breaker): AiClient {
  async function guard<T>(fn: () => Promise<T>): Promise<T> {
    let carried: { error: unknown } | null = null;
    let fnRan = false;
    let result: T;
    try {
      result = await breaker.exec(async () => {
        fnRan = true;
        try {
          return await fn();
        } catch (e) {
          if (e instanceof AiUnavailableError) throw e;
          carried = { error: e };
          return undefined as T;
        }
      });
    } catch (e) {
      if (!fnRan) throw new AiUnavailableError("circuit_open");
      throw e;
    }
    if (carried) throw (carried as { error: unknown }).error;
    return result;
  }

  return {
    structured: (req) => guard(() => ai.structured(req)),
    streamTurn: (req, onTextDelta) => guard(() => ai.streamTurn(req, onTextDelta)),
    batch: {
      submit: (items) => guard(() => ai.batch.submit(items)),
      status: (id) => guard(() => ai.batch.status(id)),
      results: (id) => ai.batch.results(id),
    },
  };
}
