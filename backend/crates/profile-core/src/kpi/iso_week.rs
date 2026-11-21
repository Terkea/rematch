//! ISO week keys (R2 Task 15).

use time::{OffsetDateTime, UtcOffset};

/// The UTC ISO 8601 week of an instant, as `"YYYY-Www"` (`"2026-W47"`). The year is the ISO
/// week-numbering year, so 2027-01-01 (a Friday) is `"2026-W53"`.
pub fn iso_week_key(d: OffsetDateTime) -> String {
    let (year, week, _) = d.to_offset(UtcOffset::UTC).to_iso_week_date();
    format!("{year:04}-W{week:02}")
}
