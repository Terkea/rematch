/** Minimal structured logger: one JSON object per line (pino-compatible shape, no dependency). */
export interface Logger {
  info(fields: Record<string, unknown>, msg: string): void;
  warn(fields: Record<string, unknown>, msg: string): void;
  error(fields: Record<string, unknown>, msg: string): void;
}

const LEVELS = { info: 30, warn: 40, error: 50 } as const;

export function createLogger(
  write: (line: string) => void = (l) => process.stdout.write(`${l}\n`),
  base: Record<string, unknown> = { service: "worker" },
): Logger {
  const silent = process.env.LOG_LEVEL === "silent";
  const emit = (level: keyof typeof LEVELS) => (fields: Record<string, unknown>, msg: string) => {
    if (silent) return;
    write(JSON.stringify({ level: LEVELS[level], time: Date.now(), ...base, ...fields, msg }));
  };
  return { info: emit("info"), warn: emit("warn"), error: emit("error") };
}

export const log: Logger = createLogger();

export const errorMessage = (err: unknown): string =>
  err instanceof Error ? err.message : String(err);
