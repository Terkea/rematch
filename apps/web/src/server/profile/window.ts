// The weakness estimate's evidence window, with no database import so client components and the
// report can name it too.

/**
 * Rust `profile_core::stats::priority::WINDOW_DAYS`: an estimate counts only the series points at
 * most this many days older than its build (`updated_at`), so its "21 of 534" is the last six
 * months, never the whole history (bug hunt I6).
 */
export const ESTIMATE_WINDOW_DAYS = 183;

/** The estimate's window as words, for the line beside its counts. */
export const ESTIMATE_WINDOW_TEXT = "in the last 6 months";
