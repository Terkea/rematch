//! The error taxonomy: by HTTP status and API `error.type`, never by message text. Nothing built
//! here carries a header, a response body or the API key.

use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex::Regex;
use time::PrimitiveDateTime;
use time::macros::format_description;

use super::port::{AiError, AiErrorKind, UnavailableReason};

static KEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sk-ant-[A-Za-z0-9_-]+").expect("valid key pattern"));

/// Replaces anything that looks like an API key with `[redacted]`.
pub fn redact(s: &str) -> String {
    KEY_PATTERN.replace_all(s, "[redacted]").into_owned()
}

/// Parses a `retry-after` value: non-negative seconds (fractions allowed, rounded to ms) or an
/// HTTP date (IMF-fixdate, `Wed, 21 Oct 2015 07:28:00 GMT`), clamped at zero.
pub fn parse_retry_after(v: Option<&str>, now: SystemTime) -> Option<Duration> {
    let v = v?.trim();
    if v.is_empty() {
        return None;
    }
    if let Ok(secs) = v.parse::<f64>() {
        if secs.is_finite() && secs >= 0.0 {
            return Some(Duration::from_millis((secs * 1000.0).round() as u64));
        }
        return None;
    }
    let fmt = format_description!(
        "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT"
    );
    let at = SystemTime::from(PrimitiveDateTime::parse(v, fmt).ok()?.assume_utc());
    Some(at.duration_since(now).unwrap_or(Duration::ZERO))
}

/// Maps an HTTP error status (and the body's `error.type`) to an [`AiError`]. `retry_after` is
/// kept only for 429. The error type is redacted because it comes from the response body.
pub fn classify_status(
    status: u16,
    api_error_type: Option<&str>,
    retry_after: Option<Duration>,
) -> AiError {
    let unavailable = |reason, retry_after| AiError::Unavailable {
        reason,
        retry_after,
    };
    if status == 529 || api_error_type == Some("overloaded_error") {
        return unavailable(UnavailableReason::Overloaded, None);
    }
    match status {
        429 => unavailable(UnavailableReason::RateLimited, retry_after),
        s if s >= 500 => unavailable(UnavailableReason::Server, None),
        401 | 403 => AiError::Auth { status },
        _ => AiError::Request {
            status,
            api_error_type: api_error_type.map(redact),
        },
    }
}

/// `api_error_type` of the fatal (non-retryable) [`AiError::Request`] for a request reqwest
/// could not build (bad URL or header), with `status: 0` since no response exists.
pub const CLIENT_BUILDER_ERROR: &str = "client_builder_error";
/// The same for a redirect policy failure (the API never redirects; a redirect is a
/// misconfigured base URL).
pub const CLIENT_REDIRECT_ERROR: &str = "client_redirect_error";

/// Maps a reqwest failure:
/// - a builder or redirect error is a configuration fault: the fatal `Request { status: 0 }`;
/// - a body that does not decode is [`AiError::Response`];
/// - every other failure (connect, timeout, a broken body stream) is `Unavailable(Connection)`.
///
/// Messages are fixed text: reqwest and serde messages can echo URLs or response content.
pub fn classify_transport(e: &reqwest::Error) -> AiError {
    if e.is_builder() || e.is_redirect() {
        let kind = if e.is_builder() {
            CLIENT_BUILDER_ERROR
        } else {
            CLIENT_REDIRECT_ERROR
        };
        return AiError::Request {
            status: 0,
            api_error_type: Some(kind.to_owned()),
        };
    }
    if e.is_decode() {
        return AiError::Response(redact("response body could not be decoded"));
    }
    AiError::Unavailable {
        reason: UnavailableReason::Connection,
        retry_after: None,
    }
}

/// Taxonomy for the `error.type` of an errored batch item.
pub fn classify_batch_error_type(t: Option<&str>) -> AiErrorKind {
    match t {
        Some("overloaded_error" | "api_error") => AiErrorKind::Retryable,
        Some("invalid_request_error" | "authentication_error" | "permission_error") => {
            AiErrorKind::Fatal
        }
        _ => AiErrorKind::Unknown,
    }
}
