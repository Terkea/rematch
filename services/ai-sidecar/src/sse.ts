/** The subset of `ServerResponse` the SSE writer uses (tests pass a fake). */
export type SseSink = {
  readonly headersSent: boolean;
  readonly writableEnded: boolean;
  writeHead(status: number, headers: Record<string, string>): unknown;
  write(chunk: string): unknown;
  end(chunk?: string): unknown;
  on(event: "close", fn: () => void): unknown;
  flushHeaders?(): void;
};

export const PING_MS = 15_000;

/** Writes one SSE stream; guarantees at most one terminal event (`done` or `error`). */
export class SseWriter {
  private ping: ReturnType<typeof setInterval> | undefined;
  private finished = false;

  constructor(private readonly res: SseSink) {}

  open(pingMs = PING_MS): void {
    this.res.writeHead(200, {
      "content-type": "text/event-stream",
      "cache-control": "no-cache",
      connection: "keep-alive",
      "x-accel-buffering": "no",
    });
    // Send the headers now: a turn can think for a while before its first event.
    this.res.flushHeaders?.();
    this.ping = setInterval(() => this.raw(": ping\n\n"), pingMs);
  }

  get done(): boolean {
    return this.finished;
  }

  event(name: "delta" | "tool_use" | "retract", data: unknown): void {
    this.raw(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`);
  }

  finish(name: "done" | "error", data: unknown): void {
    if (this.finished) return;
    this.raw(`event: ${name}\ndata: ${JSON.stringify(data)}\n\n`);
    this.close();
  }

  close(): void {
    this.finished = true;
    if (this.ping) clearInterval(this.ping);
    this.ping = undefined;
    if (!this.res.writableEnded) this.res.end();
  }

  private raw(s: string): void {
    if (!this.finished && !this.res.writableEnded) this.res.write(s);
  }
}
