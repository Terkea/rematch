import { log as defaultLog, errorMessage, type Logger } from "../log.js";

export interface ShutdownHandles {
  /** BullMQ workers: `close()` drains in-flight jobs, `close(true)` abandons them. */
  workers: Array<{ close(force?: boolean): Promise<void> }>;
  pool: { close(): Promise<void> };
  /** A drizzle postgres-js client. */
  db: { $client: { end(opts?: { timeout?: number }): Promise<void> } };
  redis: { quit(): Promise<unknown>; disconnect(): void };
  /** Queues (producers) are closed before Redis if given. */
  queues?: { close(): Promise<void> };
}

export interface ShutdownOptions {
  /** Total budget from the signal to `exit` (default 25 s). */
  graceMs?: number;
  signals?: NodeJS.Signals[];
  /** Injected for tests; defaults to the real process. */
  process?: {
    once(event: string, fn: () => void): unknown;
    on(event: string, fn: () => void): unknown;
  };
  exit?: (code: number) => void;
  log?: Logger;
}

const TIMEOUT = Symbol("timeout");

function within<T>(p: Promise<T>, ms: number): Promise<T | typeof TIMEOUT> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const t = new Promise<typeof TIMEOUT>((resolve) => {
    timer = setTimeout(() => resolve(TIMEOUT), Math.max(0, ms));
  });
  return Promise.race([p, t]).finally(() => clearTimeout(timer));
}

/**
 * Closes everything in order and resolves with the exit code (0, or 1 if anything failed).
 * 1. Workers stop taking jobs and drain in-flight ones, for up to 60% of `graceMs`.
 * 2. If they have not drained, the engine pool is closed. In-flight analyses fail with
 *    EnginePoolClosedError, put their game back to `pending` and BullMQ schedules a retry.
 * 3. Workers still not closed by 90% of `graceMs` are force-closed. Their jobs' locks expire and
 *    stalled-job detection hands them to another worker.
 * 4. Pool, queues, db and Redis are closed, each failure logged and remembered.
 */
export async function shutdown(
  handles: ShutdownHandles,
  opts: ShutdownOptions = {},
): Promise<number> {
  const log = opts.log ?? defaultLog;
  const graceMs = opts.graceMs ?? 25_000;
  const started = Date.now();
  const remaining = (fraction: number) => started + graceMs * fraction - Date.now();
  let failed = false;
  const attempt = async (name: string, fn: () => Promise<unknown>, budgetMs: number) => {
    try {
      const r = await within(fn(), budgetMs);
      if (r === TIMEOUT) {
        failed = true;
        log.error({ resource: name }, "close timed out");
      }
    } catch (err) {
      failed = true;
      log.error({ resource: name, err: errorMessage(err) }, "close failed");
    }
  };

  const draining = handles.workers.map((w) =>
    w.close().then(
      () => true,
      (err) => {
        failed = true;
        log.error({ resource: "worker", err: errorMessage(err) }, "close failed");
        return true;
      },
    ),
  );
  const drained = await within(Promise.all(draining), remaining(0.6));
  let poolClosed = false;
  if (drained === TIMEOUT) {
    log.warn({ graceMs }, "workers did not drain; aborting in-flight analyses");
    poolClosed = true;
    await attempt("pool", () => handles.pool.close(), remaining(0.9));
    const settled = await within(Promise.all(draining), remaining(0.9));
    if (settled === TIMEOUT) {
      log.warn({}, "force-closing workers; stalled-job detection will hand their jobs on");
      await Promise.all(
        handles.workers.map((w) => attempt("worker", () => w.close(true), remaining(0.95))),
      );
    }
  }
  const tail = () => Math.max(100, remaining(1));
  if (!poolClosed) await attempt("pool", () => handles.pool.close(), tail());
  if (handles.queues) {
    const queues = handles.queues;
    await attempt("queues", () => queues.close(), tail());
  }
  await attempt("db", () => handles.db.$client.end({ timeout: 5 }), tail());
  try {
    const r = await within(handles.redis.quit(), tail());
    if (r === TIMEOUT) handles.redis.disconnect();
  } catch (err) {
    failed = true;
    log.error({ resource: "redis", err: errorMessage(err) }, "close failed");
    handles.redis.disconnect();
  }
  log.info({ ms: Date.now() - started, ok: !failed }, "shutdown complete");
  return failed ? 1 : 0;
}

/** On SIGTERM/SIGINT, runs `shutdown` once (later signals are ignored) and exits with its code. */
export function installGracefulShutdown(
  handles: ShutdownHandles,
  opts: ShutdownOptions = {},
): void {
  const proc = opts.process ?? process;
  const exit = opts.exit ?? ((code: number) => process.exit(code));
  const log = opts.log ?? defaultLog;
  let running = false;
  const onSignal = (signal: string) => () => {
    if (running) {
      log.warn({ signal }, "shutdown already in progress");
      return;
    }
    running = true;
    log.info({ signal }, "shutting down");
    void shutdown(handles, opts).then(exit, (err) => {
      log.error({ err: errorMessage(err) }, "shutdown crashed");
      exit(1);
    });
  };
  for (const s of opts.signals ?? ["SIGTERM", "SIGINT"]) proc.on(s, onSignal(s));
}
