export class PlatformNotFoundError extends Error {
  constructor(url: string) {
    super(`Not found: ${url}`);
    this.name = "PlatformNotFoundError";
  }
}

export class PlatformRateLimitError extends Error {
  constructor(url: string, attempts: number) {
    super(`Rate limited (HTTP 429) after ${attempts} attempts: ${url}`);
    this.name = "PlatformRateLimitError";
  }
}

export class PlatformTimeoutError extends Error {
  constructor(url: string, ms: number) {
    super(`No response within ${ms} ms: ${url}`);
    this.name = "PlatformTimeoutError";
  }
}

export interface RequestInitLite {
  headers?: Record<string, string>;
}

export interface PlatformClient {
  getJson<T>(url: string, init?: RequestInitLite): Promise<T>;
  getNdjson<T>(url: string, init?: RequestInitLite): AsyncIterable<T>;
}

export const USER_AGENT = "rematch/0.1 (+https://github.com/Terkea/rematch)";

export interface PlatformClientOptions {
  name: "lichess" | "chesscom";
  fetch?: typeof fetch;
  retryAfterMs?: number;
  maxRetries?: number;
  /** Per-request timeout for headers (and the whole body for getJson). Default 30 s. */
  requestTimeoutMs?: number;
  /** getNdjson: maximum silence between chunks (not a total cap). Default 60 s. */
  streamIdleTimeoutMs?: number;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** A re-armable deadline that aborts a fetch and rejects whatever it is raced against. */
function createDeadline(url: string) {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let fail!: (err: Error) => void;
  const expired = new Promise<never>((_, reject) => {
    fail = reject;
  });
  expired.catch(() => {});
  return {
    signal: controller.signal,
    arm(ms: number) {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const err = new PlatformTimeoutError(url, ms);
        fail(err);
        controller.abort(err);
      }, ms);
    },
    clear() {
      clearTimeout(timer);
    },
    race<T>(work: Promise<T>): Promise<T> {
      return Promise.race([work, expired]);
    },
  };
}

type Deadline = ReturnType<typeof createDeadline>;

export function createPlatformClient(opts: PlatformClientOptions): PlatformClient {
  const fetchFn = opts.fetch ?? fetch;
  const retryAfterMs = opts.retryAfterMs ?? 60_000;
  const maxRetries = opts.maxRetries ?? 3;
  const requestTimeoutMs = opts.requestTimeoutMs ?? 30_000;
  const streamIdleTimeoutMs = opts.streamIdleTimeoutMs ?? 60_000;
  const userAgent = USER_AGENT;

  // Promise-chain mutex: one request at a time per client (platform).
  let tail: Promise<void> = Promise.resolve();
  const acquire = async (): Promise<() => void> => {
    let release!: () => void;
    const next = new Promise<void>((resolve) => {
      release = resolve;
    });
    const previous = tail;
    tail = previous.then(() => next);
    await previous;
    return release;
  };

  // Must be called while holding the lock. Retries 429 responses. On success the
  // returned deadline is still armed (covering the body); the caller clears it.
  async function request(
    url: string,
    init?: RequestInitLite,
  ): Promise<{ res: Response; deadline: Deadline }> {
    for (let attempt = 0; ; attempt++) {
      const deadline = createDeadline(url);
      deadline.arm(requestTimeoutMs);
      try {
        const res = await deadline.race(
          fetchFn(url, {
            headers: { ...init?.headers, "User-Agent": userAgent },
            signal: deadline.signal,
          }),
        );
        if (res.status === 429) {
          await res.body?.cancel().catch(() => {});
          if (attempt >= maxRetries) throw new PlatformRateLimitError(url, attempt + 1);
        } else if (res.status === 404) {
          await res.body?.cancel().catch(() => {});
          throw new PlatformNotFoundError(url);
        } else if (!res.ok) {
          await res.body?.cancel().catch(() => {});
          throw new Error(`${opts.name} request failed with HTTP ${res.status}: ${url}`);
        } else {
          return { res, deadline };
        }
      } catch (err) {
        deadline.clear();
        throw err;
      }
      deadline.clear();
      await sleep(retryAfterMs);
    }
  }

  return {
    async getJson<T>(url: string, init?: RequestInitLite): Promise<T> {
      const release = await acquire();
      try {
        const { res, deadline } = await request(url, init);
        try {
          return (await deadline.race(res.json())) as T;
        } finally {
          deadline.clear();
        }
      } finally {
        release();
      }
    },

    async *getNdjson<T>(url: string, init?: RequestInitLite): AsyncIterable<T> {
      const release = await acquire();
      try {
        const { res, deadline } = await request(url, init);
        if (!res.body) {
          deadline.clear();
          return;
        }
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        try {
          for (;;) {
            deadline.arm(streamIdleTimeoutMs);
            const { done, value } = await deadline.race(reader.read());
            deadline.clear();
            if (done) break;
            buffer += value;
            let nl = buffer.indexOf("\n");
            while (nl !== -1) {
              const line = buffer.slice(0, nl).trim();
              buffer = buffer.slice(nl + 1);
              if (line) yield JSON.parse(line) as T;
              nl = buffer.indexOf("\n");
            }
          }
          const rest = buffer.trim();
          if (rest) yield JSON.parse(rest) as T;
        } finally {
          deadline.clear();
          await reader.cancel().catch(() => {});
        }
      } finally {
        release();
      }
    },
  };
}
