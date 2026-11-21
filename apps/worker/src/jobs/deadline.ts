/** Hard cap on one analysis attempt. */
export const ANALYSE_DEADLINE_MS = 10 * 60_000;
/** An import that makes no progress (no game streamed) for this long is abandoned. */
export const IMPORT_IDLE_DEADLINE_MS = 30 * 60_000;

/**
 * A job overran its deadline. Thrown even while BullMQ still holds a live lock (the worker is
 * alive but its work is stuck), so the attempt ends and the normal retry path takes over.
 */
export class JobDeadlineError extends Error {
  constructor(
    readonly kind: "analyse-game" | "import-account",
    readonly ms: number,
  ) {
    super(`${kind} exceeded its ${ms} ms deadline`);
    this.name = "JobDeadlineError";
  }
}

export interface Deadline {
  signal: AbortSignal;
  /** Re-arms the timer (for idle deadlines). */
  reset(): void;
  clear(): void;
}

export function createDeadline(kind: JobDeadlineError["kind"], ms: number): Deadline {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const arm = () => {
    clearTimeout(timer);
    timer = setTimeout(() => controller.abort(new JobDeadlineError(kind, ms)), ms);
    timer.unref?.();
  };
  arm();
  return {
    signal: controller.signal,
    reset() {
      if (!controller.signal.aborted) arm();
    },
    clear() {
      clearTimeout(timer);
    },
  };
}

/** Resolves or rejects like `work`, or rejects with the signal's reason as soon as it aborts. */
export function abortable<T>(work: Promise<T>, signal: AbortSignal | undefined): Promise<T> {
  if (!signal) return work;
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(signal.reason);
    signal.addEventListener("abort", onAbort, { once: true });
    work.then(
      (v) => {
        signal.removeEventListener("abort", onAbort);
        resolve(v);
      },
      (e) => {
        signal.removeEventListener("abort", onAbort);
        reject(e);
      },
    );
  });
}
