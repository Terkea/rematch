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

export interface RequestInitLite {
  headers?: Record<string, string>;
}

export interface PlatformClient {
  getJson<T>(url: string, init?: RequestInitLite): Promise<T>;
  getNdjson<T>(url: string, init?: RequestInitLite): AsyncIterable<T>;
}

export interface PlatformClientOptions {
  name: "lichess" | "chesscom";
  contactEmail: string;
  fetch?: typeof fetch;
  retryAfterMs?: number;
  maxRetries?: number;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export function createPlatformClient(opts: PlatformClientOptions): PlatformClient {
  const fetchFn = opts.fetch ?? fetch;
  const retryAfterMs = opts.retryAfterMs ?? 60_000;
  const maxRetries = opts.maxRetries ?? 3;
  const userAgent = `chess-improver/0.1 (+mailto:${opts.contactEmail})`;

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

  // Must be called while holding the lock. Retries 429 responses.
  async function request(url: string, init?: RequestInitLite): Promise<Response> {
    for (let attempt = 0; ; attempt++) {
      const res = await fetchFn(url, {
        headers: { ...init?.headers, "User-Agent": userAgent },
      });
      if (res.status === 429) {
        await res.body?.cancel();
        if (attempt >= maxRetries) throw new PlatformRateLimitError(url, attempt + 1);
        await sleep(retryAfterMs);
        continue;
      }
      if (res.status === 404) {
        await res.body?.cancel();
        throw new PlatformNotFoundError(url);
      }
      if (!res.ok) {
        await res.body?.cancel();
        throw new Error(`${opts.name} request failed with HTTP ${res.status}: ${url}`);
      }
      return res;
    }
  }

  return {
    async getJson<T>(url: string, init?: RequestInitLite): Promise<T> {
      const release = await acquire();
      try {
        const res = await request(url, init);
        return (await res.json()) as T;
      } finally {
        release();
      }
    },

    async *getNdjson<T>(url: string, init?: RequestInitLite): AsyncIterable<T> {
      const release = await acquire();
      try {
        const res = await request(url, init);
        if (!res.body) return;
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        try {
          for (;;) {
            const { done, value } = await reader.read();
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
          await reader.cancel().catch(() => {});
        }
      } finally {
        release();
      }
    },
  };
}
