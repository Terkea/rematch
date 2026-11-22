// Better Auth's logger, with every email address replaced (T21 review Minor 7, T27). Better Auth
// logs a failed database write together with the row it tried to write, and a magic-link
// verification row's value holds the address; no address may reach the server log.

const EMAIL = /[\w.+%-]+@[\w-]+(?:\.[\w-]+)+/g;

const redact = (text: string): string => text.replace(EMAIL, "[email]");

function clean(value: unknown, depth = 0): unknown {
  if (typeof value === "string") return redact(value);
  if (value instanceof Error) {
    const copy = new Error(redact(value.message));
    copy.name = value.name;
    copy.stack = value.stack === undefined ? undefined : redact(value.stack);
    return copy;
  }
  if (value === null || typeof value !== "object" || depth > 4) return value;
  try {
    // Objects go through JSON so nested rows, arrays and cause chains are all covered.
    return JSON.parse(redact(JSON.stringify(value)));
  } catch {
    return "[unprintable]";
  }
}

export function redactedLog(
  level: "debug" | "info" | "warn" | "error",
  message: string,
  ...args: unknown[]
): void {
  const write = level === "debug" ? console.debug : console[level];
  write(`[auth] ${redact(message)}`, ...args.map((arg) => clean(arg)));
}
