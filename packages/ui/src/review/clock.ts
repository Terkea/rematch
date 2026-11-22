// Clock text for the review (Review Focus 5: bullet to daily games). Every value is floored, so a
// clock never reads more time than remained. A value that is not a finite number has no honest
// text: formatClock gives "" and the components draw nothing for it, never 0.

const TENTHS_BELOW = 20;
const HOUR = 3600;
const DAY = 86_400;

const pad = (n: number) => String(n).padStart(2, "0");

/** Whole tenths, floored, with a nudge for binary fractions (8.3 × 10 is 82.99…). */
const tenthsOf = (seconds: number) => Math.floor(seconds * 10 + 1e-6);

/**
 * `0:08.4` below 20 s, `9:05` below an hour, `1:02:03` below a day, and `2 d 3 h` from a day up.
 * Negative input clamps to 0.
 */
export interface ClockFormat {
  /**
   * Show tenths below 20 s (default true). Pass false for a source that records whole seconds
   * only, so the clock never claims precision it doesn't have.
   */
  tenths?: boolean;
}

export function formatClock(seconds: number, { tenths = true }: ClockFormat = {}): string {
  if (!Number.isFinite(seconds)) return "";
  const s = Math.max(0, seconds);
  if (tenths && s < TENTHS_BELOW) {
    const t = tenthsOf(s);
    return `0:${pad(Math.floor(t / 10))}.${t % 10}`;
  }
  const whole = Math.floor(s);
  if (whole < HOUR) return `${Math.floor(whole / 60)}:${pad(whole % 60)}`;
  if (whole < DAY) {
    return `${Math.floor(whole / HOUR)}:${pad(Math.floor((whole % HOUR) / 60))}:${pad(whole % 60)}`;
  }
  return `${Math.floor(whole / DAY)} d ${Math.floor((whole % DAY) / HOUR)} h`;
}

/** The same floored time as an ISO 8601 duration, for `<time dateTime>`: `PT9M5S`, `PT8.4S`. */
export function clockDuration(seconds: number, { tenths = true }: ClockFormat = {}): string {
  const s = Math.max(0, Number.isFinite(seconds) ? seconds : 0);
  if (tenths && s < TENTHS_BELOW) return `PT${tenthsOf(s) / 10}S`;
  const whole = Math.floor(s);
  const h = Math.floor(whole / HOUR);
  const m = Math.floor((whole % HOUR) / 60);
  const sec = whole % 60;
  return `PT${h ? `${h}H` : ""}${m ? `${m}M` : ""}${sec || (!h && !m) ? `${sec}S` : ""}`;
}

/** `12.4 s on this move` (floored to tenths, or whole seconds), or the clock format from a minute up. */
export function formatSpent(seconds: number, { tenths = true }: ClockFormat = {}): string | null {
  if (!Number.isFinite(seconds) || seconds < 0) return null;
  if (seconds < 60) {
    return tenths
      ? `${(tenthsOf(seconds) / 10).toFixed(1)} s on this move`
      : `${Math.floor(seconds)} s on this move`;
  }
  return `${formatClock(seconds)} on this move`;
}
