// Format keys and labels (Q2): pure, with no database import, so client code (the games
// explorer's links) can read and write `fmt`. The queries live in `server/filters/formats.ts`.

/** The `fmt` value of every game outside a class's top formats. */
export const OTHER_FORMAT = "other";
/** Formats listed by name per time class; the rest are "Other". */
export const TOP_FORMATS = 5;

const KEY_RE = /^(\d{1,6})\+(\d{1,4})$/;

/** `600+0` for 600 s and no increment (a missing increment is 0); null without a base. */
export function formatKey(base: number | null, increment: number | null): string | null {
  if (base == null || !Number.isSafeInteger(base) || base < 0) return null;
  const inc = increment != null && Number.isSafeInteger(increment) && increment > 0 ? increment : 0;
  return `${base}+${inc}`;
}

/** `{ base, increment }` of a valid key, normalised (`0600+00` is `600+0`); null otherwise. */
export function parseFormatKey(key: string): { base: number; increment: number } | null {
  const m = KEY_RE.exec(key);
  if (!m) return null;
  return { base: Number(m[1]), increment: Number(m[2]) };
}

/** A base in seconds as players say it: 600 is "10", 90 is "1.5", 30 is "30s". */
function baseLabel(seconds: number): string {
  if (seconds % 60 === 0) return String(seconds / 60);
  if (seconds < 60) return `${seconds}s`;
  return String(Math.round((seconds / 60) * 10) / 10);
}

/** "10+0" for `600+0`, "Other" for `other`; an invalid key is returned as is. */
export function formatLabel(key: string): string {
  if (key === OTHER_FORMAT) return "Other";
  const f = parseFormatKey(key);
  return f ? `${baseLabel(f.base)}+${f.increment}` : key;
}
