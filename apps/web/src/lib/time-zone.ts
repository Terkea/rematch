// The player's time zone (R2-8): no user row stores one, so the browser sends its `Intl` zone as
// a first-party cookie, set on the first signed-in load. A request without it, or with a zone the
// runtime does not know, counts days in UTC. Client-safe: no server import.

export const TIME_ZONE_COOKIE = "ci_tz";

/**
 * An IANA time zone the runtime knows (the browser's `Intl` zone, sent as a cookie), else UTC,
 * the fallback for a player who has not sent one.
 */
export function timeZoneOf(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 64) return "UTC";
  try {
    return new Intl.DateTimeFormat("en-GB", { timeZone: v }).resolvedOptions().timeZone;
  } catch {
    return "UTC";
  }
}

/** The zone in a `Cookie` header, else UTC. */
export function timeZoneFromCookieHeader(header: string | null | undefined): string {
  if (!header) return "UTC";
  for (const part of header.split(";")) {
    const eq = part.indexOf("=");
    if (eq < 0 || part.slice(0, eq).trim() !== TIME_ZONE_COOKIE) continue;
    try {
      return timeZoneOf(decodeURIComponent(part.slice(eq + 1).trim()));
    } catch {
      return "UTC";
    }
  }
  return "UTC";
}
