//! The AI retry schedule. A transient AI failure (unavailable, rate limited, open breaker) does
//! not use a queue attempt: the handler records the retry on its row and returns
//! `JobError::Release { delay: retry_delay(..) }`. After the fifth retry the item fails.

use std::time::Duration;

/// Delay before retry `n` (1-based): 5 min, 30 min, 2 h, 6 h, 24 h.
pub const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(300),
    Duration::from_secs(1800),
    Duration::from_secs(7200),
    Duration::from_secs(21600),
    Duration::from_secs(86400),
];

/// A server `retry-after` is honoured up to this cap.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(86400);

/// The delay before retry `retry_count` (the count after incrementing, so the first retry is 1):
/// `max(RETRY_DELAYS[n - 1], min(retry_after, 24 h))`, or `None` once the schedule is exhausted
/// (after 5). A count below 1 is treated as 1.
pub fn retry_delay(retry_count: i32, retry_after: Option<Duration>) -> Option<Duration> {
    let idx = usize::try_from(retry_count.max(1) - 1).ok()?;
    let base = *RETRY_DELAYS.get(idx)?;
    Some(retry_after.map_or(base, |ra| base.max(ra.min(MAX_RETRY_AFTER))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_treated_as_the_first_retry() {
        assert_eq!(retry_delay(0, None), Some(RETRY_DELAYS[0]));
        assert_eq!(retry_delay(-3, None), Some(RETRY_DELAYS[0]));
    }
}
