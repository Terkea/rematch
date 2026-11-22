import { ClientGone, Overloaded } from "./errors.js";

type Waiter = { start: () => void; fail: (e: Error) => void };

const NEVER = new AbortController().signal;

/**
 * FIFO concurrency cap. Each run is one `claude` subprocess (about 1 GiB), so excess requests wait
 * in a bounded queue for at most `waitMs`, then fail with `Overloaded` (529, retry-after 5).
 */
export class Limiter {
  private inFlight = 0;
  private readonly queue: Waiter[] = [];

  constructor(
    readonly max: number,
    private readonly maxQueue: number,
    private readonly waitMs: number,
  ) {}

  stats(): { inFlight: number; queued: number } {
    return { inFlight: this.inFlight, queued: this.queue.length };
  }

  async run<T>(
    fn: (signal: AbortSignal) => Promise<T>,
    opts: { signal?: AbortSignal } = {},
  ): Promise<T> {
    const signal = opts.signal ?? NEVER;
    if (signal.aborted) throw new ClientGone();
    if (this.inFlight >= this.max) await this.wait(signal);
    else this.inFlight += 1;
    try {
      return await fn(signal);
    } finally {
      this.release();
    }
  }

  /** Resolves once a slot has been handed to this waiter (inFlight already counts it). */
  private wait(signal: AbortSignal): Promise<void> {
    if (this.queue.length >= this.maxQueue) return Promise.reject(new Overloaded());
    return new Promise<void>((resolve, reject) => {
      const leave = (e: Error) => {
        const i = this.queue.indexOf(waiter);
        if (i >= 0) this.queue.splice(i, 1);
        cleanup();
        reject(e);
      };
      const onAbort = () => leave(new ClientGone());
      const timer = setTimeout(() => leave(new Overloaded()), this.waitMs);
      const cleanup = () => {
        clearTimeout(timer);
        signal.removeEventListener("abort", onAbort);
      };
      const waiter: Waiter = {
        start: () => {
          cleanup();
          resolve();
        },
        fail: leave,
      };
      signal.addEventListener("abort", onAbort, { once: true });
      this.queue.push(waiter);
    });
  }

  private release(): void {
    const next = this.queue.shift();
    // The slot passes straight to the next waiter, if any.
    if (next) next.start();
    else this.inFlight -= 1;
  }
}
