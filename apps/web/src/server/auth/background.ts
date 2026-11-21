const pending = new Set<Promise<void>>();

/**
 * Runs `task` after the current response, so the request path (and its timing) never depends on
 * what the task does. Failures are logged, never thrown into the request.
 */
export function runAfterResponse(label: string, task: () => Promise<void>): void {
  const run = new Promise<void>((resolve) => setImmediate(resolve))
    .then(task)
    .catch((error: unknown) => {
      console.error(`[auth] ${label} failed`, error);
    })
    .finally(() => {
      pending.delete(run);
    });
  pending.add(run);
}

/** Number of tasks still running (tests). */
export function pendingBackgroundTasks(): number {
  return pending.size;
}

/** Waits until every queued task has finished (tests). */
export async function flushBackgroundTasks(): Promise<void> {
  while (pending.size > 0) await Promise.all([...pending]);
}
