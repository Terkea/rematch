/**
 * Structured logs. Callers pass only the whitelisted fields below, never a prompt, an output, a
 * token or a key, so a log line cannot carry one by accident.
 */
export type LogFields = {
  msg: string;
  route?: string;
  model?: string;
  subtype?: string;
  numTurns?: number;
  durationMs?: number;
  status?: number;
  session?: string;
  inputTokens?: number;
  outputTokens?: number;
  cacheReadInputTokens?: number;
  cacheCreationInputTokens?: number;
  event?: string;
};

export type Logger = {
  info(f: LogFields): void;
  warn(f: LogFields): void;
  error(f: LogFields): void;
};

const line = (level: string) => (f: LogFields) => {
  process.stdout.write(`${JSON.stringify({ level, time: new Date().toISOString(), ...f })}\n`);
};

export const stdoutLogger: Logger = {
  info: line("info"),
  warn: line("warn"),
  error: line("error"),
};

/** Process counters surfaced on /health. */
export class Metrics {
  private mirrorErrors = 0;
  private nulStripped = 0;
  mirrorError(): void {
    this.mirrorErrors += 1;
  }
  /** Transcript entries that had U+0000 removed before they reached Postgres. */
  nulStrippedEntries(n: number): void {
    this.nulStripped += n;
  }
  snapshot(): { mirrorErrors: number; nulStrippedEntries: number } {
    return { mirrorErrors: this.mirrorErrors, nulStrippedEntries: this.nulStripped };
  }
}
