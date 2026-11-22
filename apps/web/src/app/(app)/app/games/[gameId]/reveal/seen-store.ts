// R10 (ADR 0052): which games this device has seen revealed. `localStorage["ci.review.seen"]` holds
// a JSON array of at most 500 ids, oldest first. Storage can be missing, blocked or full (private
// windows, cleared site data): every access is guarded, and a failure reads as "seen", so a broken
// store gives the short version every time instead of the full one every time.

export const SEEN_KEY = "ci.review.seen";
export const SEEN_MAX = 500;

function read(): string[] {
  const raw = window.localStorage.getItem(SEEN_KEY);
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
  } catch {
    // A corrupt value is treated as empty; the next add rewrites it.
    return [];
  }
}

export const seenStore = {
  has(id: string): boolean {
    try {
      return read().includes(id);
    } catch {
      return true;
    }
  },
  /**
   * Stores the id; false when it could not be stored. A store that reads but cannot write (a full
   * quota, old Safari private mode) would answer "not seen" forever, so the caller treats a failed
   * add as seen too.
   */
  add(id: string): boolean {
    try {
      const ids = read().filter((x) => x !== id);
      ids.push(id);
      window.localStorage.setItem(SEEN_KEY, JSON.stringify(ids.slice(-SEEN_MAX)));
      return true;
    } catch {
      return false;
    }
  },
};
