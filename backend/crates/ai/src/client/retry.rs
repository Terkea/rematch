//! Retry policy shared by the sidecar and batch clients (ADR-R3-1).
//!
//! Retryable: a connection failure (no status), 408, 409, 429, every 5xx and 529. An
//! `x-should-retry: true|false` header from the API overrides the status. The delay is the
//! server's `retry-after` when it is at or under the cap; over the cap the caller gives up at once
//! and surfaces `retry_after` (the job queue reschedules). Without one it is exponential backoff,
//! `base * 2^attempt`, plus 0 to 250 ms of jitter.

use std::time::Duration;

/// Upper bound of the random jitter added to a backoff delay.
pub const MAX_JITTER: Duration = Duration::from_millis(250);

/// Whether a failed call may be retried. `status` is `None` for a connection failure.
pub fn should_retry(status: Option<u16>, x_should_retry: Option<&str>) -> bool {
    match x_should_retry.map(str::trim) {
        Some("true") => return true,
        Some("false") => return false,
        _ => {}
    }
    match status {
        None => true,
        Some(s) => matches!(s, 408 | 409 | 429) || s >= 500,
    }
}

/// The delay before retry number `attempt + 1` (`attempt` counts from 0), or `None` when the
/// server asked for longer than `cap`.
pub fn retry_delay(
    attempt: u32,
    retry_after: Option<Duration>,
    cap: Duration,
    base: Duration,
) -> Option<Duration> {
    if let Some(ra) = retry_after {
        return (ra <= cap).then_some(ra);
    }
    let factor = 1u32 << attempt.min(16);
    let jitter = Duration::from_millis(rand::random_range(0..=MAX_JITTER.as_millis() as u64));
    Some(base.saturating_mul(factor).saturating_add(jitter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses() {
        for s in [408, 409, 429, 500, 502, 503, 504, 529] {
            assert!(should_retry(Some(s), None), "{s}");
        }
        for s in [400, 401, 403, 404, 413] {
            assert!(!should_retry(Some(s), None), "{s}");
        }
        assert!(should_retry(None, None));
        assert!(!should_retry(Some(529), Some("false")));
        assert!(should_retry(Some(400), Some("true")));
    }

    #[test]
    fn delays() {
        let cap = Duration::from_secs(30);
        let base = Duration::from_millis(500);
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(7)), cap, base),
            Some(Duration::from_secs(7))
        );
        assert_eq!(retry_delay(0, Some(cap), cap, base), Some(cap));
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(31)), cap, base),
            None
        );
        for attempt in 0..4 {
            let d = retry_delay(attempt, None, cap, base).unwrap();
            let floor = base * 2u32.pow(attempt);
            assert!(d >= floor && d <= floor + MAX_JITTER, "{attempt}: {d:?}");
        }
        assert!(
            retry_delay(u32::MAX, None, cap, base).is_some(),
            "no overflow"
        );
    }
}
