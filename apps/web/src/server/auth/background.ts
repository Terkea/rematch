/** At most this many tasks are queued or running; more are dropped with a warning. */
export const MAX_PENDING_TASKS = 100;
/** At most this many tasks run at once. */
export const MAX_RUNNING_TASKS = 4;

type Job = { label: string; task: () => Promise<void>; done: () => void };

const queue: Job[] = [];
const inFlight = new Set<Promise<void>>();
let running = 0;
let dropped = 0;
/** Task errors kept for `flushBackgroundTasks` to rethrow; only collected under test. */
const errors: unknown[] = [];
const collectErrors = process.env.NODE_ENV === "test" || Boolean(process.env.VITEST);

function pump(): void {
  while (running < MAX_RUNNING_TASKS && queue.length > 0) {
    const job = queue.shift() as Job;
    running += 1;
    job
      .task()
      .catch((error: unknown) => {
        console.error(`[auth] ${job.label} failed`, error);
        if (collectErrors) errors.push(error);
      })
      .finally(() => {
        running -= 1;
        job.done();
        pump();
      });
  }
}

/**
 * Runs `task` after the current response, so the request path (and its timing) never depends on
 * what the task does. The queue is bounded (`MAX_PENDING_TASKS`, extra tasks are dropped and
 * counted) and runs at most `MAX_RUNNING_TASKS` at once. Failures are logged, never thrown into
 * the request. Returns false when the task was dropped.
 */
export function runAfterResponse(label: string, task: () => Promise<void>): boolean {
  if (inFlight.size >= MAX_PENDING_TASKS) {
    dropped += 1;
    console.warn(`[auth] background queue full (${MAX_PENDING_TASKS}); dropped ${label}`);
    return false;
  }
  let done = () => {};
  const finished = new Promise<void>((resolve) => {
    done = resolve;
  });
  inFlight.add(finished);
  void finished.then(() => inFlight.delete(finished));
  setImmediate(() => {
    queue.push({ label, task, done });
    pump();
  });
  return true;
}

/** Tasks queued or running (tests and diagnostics). */
export function pendingBackgroundTasks(): number {
  return inFlight.size;
}

/** Tasks dropped because the queue was full, since start. */
export function droppedBackgroundTasks(): number {
  return dropped;
}

/**
 * Waits until every queued task has finished. Under test it then rethrows the first task error
 * (and forgets them all), so a broken delivery fails the test instead of only logging.
 */
export async function flushBackgroundTasks(): Promise<void> {
  while (inFlight.size > 0) await Promise.all([...inFlight]);
  if (errors.length > 0) {
    const [first] = errors.splice(0, errors.length);
    throw first;
  }
}
