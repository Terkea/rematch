// Logging with every email address replaced (T21 review Minor 7, review-t27 I1). Better Auth logs
// a failed database write together with the row it tried to write, a Drizzle query error carries
// its bound parameters (the address), and a nodemailer error its recipients. No address may reach
// the server log, so Better Auth's logger and the background-task logs both go through here.

// A local part (anything but whitespace and the characters that delimit an address in logs),
// "@", then a domain of letters, digits and hyphens in any script, dotted or not ("localhost").
const EMAIL = /[^\s<>()[\]{},;:"@]+@[\p{L}\p{N}-]+(?:\.[\p{L}\p{N}-]+)*/gu;

/** Deepest nesting walked; anything below is replaced, never printed raw. */
const MAX_DEPTH = 6;

export const redact = (text: string): string => text.replace(EMAIL, "[email]");

/**
 * A copy of `value` with every string redacted: object fields, array items, and an Error's
 * message, stack, `cause` and own fields (a pg `code` or `detail`), which are kept for diagnosis.
 */
export function redactValue(value: unknown, depth = 0, seen = new WeakSet<object>()): unknown {
  if (typeof value === "string") return redact(value);
  if (value === null || typeof value !== "object") return value;
  if (seen.has(value)) return "[circular]";
  if (depth >= MAX_DEPTH) return "[nested]";
  seen.add(value);
  const next = (v: unknown) => redactValue(v, depth + 1, seen);
  if (value instanceof Error) {
    const copy = new Error(redact(value.message));
    copy.name = value.name;
    copy.stack = value.stack === undefined ? undefined : redact(value.stack);
    if (value.cause !== undefined) copy.cause = next(value.cause);
    for (const [key, field] of Object.entries(value)) {
      if (key !== "cause") (copy as unknown as Record<string, unknown>)[key] = next(field);
    }
    return copy;
  }
  if (Array.isArray(value)) return value.map(next);
  return Object.fromEntries(Object.entries(value).map(([key, field]) => [key, next(field)]));
}

export function redactedLog(
  level: "debug" | "info" | "warn" | "error",
  message: string,
  ...args: unknown[]
): void {
  const write = level === "debug" ? console.debug : console[level];
  write(`[auth] ${redact(message)}`, ...args.map((arg) => redactValue(arg)));
}
