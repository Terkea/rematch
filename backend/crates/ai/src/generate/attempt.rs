//! Task 14's attempt, validate and repair-once flow, for the calls that are not report sections
//! (Task 15: the weekly review and change notes). The rules are [`super::section`]'s:
//!
//! 1. Attempt 1, after the budget hook. A result without parsed output is unusable
//!    ([`parse_errors`]: `refusal`, `truncated` or `schema_invalid`).
//! 2. A clean result is `Ready` after one call.
//! 3. Otherwise exactly one repair: the same request plus [`build_repair_messages`] (the raw
//!    answer, then the errors as `- path: code: detail`), or the plain request again when the
//!    first answer has no content to replay.
//! 4. After the repair, failing items are dropped and the rest is `Ready`. An unusable repair
//!    falls back to attempt 1's validated items when attempt 1 was only partly invalid (the T14
//!    m1 ruling); otherwise the result is `Dropped`, with one [`DroppedItem`] at path `""`.
//!
//! Client errors propagate as a [`SectionError`] carrying the records of the calls already paid
//! for, so they are always metered. Nothing that failed the validator is ever returned.

use serde_json::Value;

use super::request::build_repair_messages;
use super::section::{BeforeCall, CallRecord, SectionError, SectionStatus, call, parse_errors};
use crate::client::port::{AiClient, AiError, StructuredRequest, StructuredResult};
use crate::output::codes::{DroppedItem, ValidationError};

/// How one result fared.
pub(crate) enum Checked<T> {
    /// Validated with no error at all.
    Clean(T),
    /// Validated, but some items failed (they are already left out of `T`).
    Partial(T, Vec<DroppedItem>),
    /// Nothing usable: no parsed output, or nothing that validated.
    Unusable(Vec<ValidationError>),
}

/// The parsed output of `r`, or its parse errors.
pub(crate) fn parsed(r: &StructuredResult) -> Result<&Value, Vec<ValidationError>> {
    r.parsed
        .as_ref()
        .filter(|_| r.parse_error.is_none())
        .ok_or_else(|| parse_errors(r))
}

/// The outcome of [`attempt_and_repair`]. `content` is `Some` exactly when `status` is `Ready`.
pub(crate) struct Attempted<T> {
    pub status: SectionStatus,
    pub content: Option<T>,
    pub dropped: Vec<DroppedItem>,
    /// Every call made, in order: 1 or 2.
    pub calls: Vec<CallRecord>,
}

/// Runs `req`, checks it with `check`, and repairs once (see the module docs). `label` is the
/// `DroppedItem::section` of a whole-result drop (`"weekly"`, `"change"`).
pub(crate) async fn attempt_and_repair<T>(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    req: &StructuredRequest,
    label: &str,
    check: impl Fn(&StructuredResult) -> Checked<T>,
) -> Result<Attempted<T>, SectionError> {
    let first = call(ai, before_call, req).await?;
    let calls = CallRecord::from_result(&first, false);
    let errors = match check(&first) {
        Checked::Clean(content) => {
            return Ok(Attempted {
                status: SectionStatus::Ready,
                content: Some(content),
                dropped: Vec::new(),
                calls,
            });
        }
        Checked::Partial(_, items) => items.into_iter().flat_map(|d| d.errors).collect(),
        Checked::Unusable(errors) => errors,
    };
    match repair_once(ai, before_call, req, label, &first, errors, &check).await {
        Ok(mut out) => {
            out.calls.splice(0..0, calls);
            Ok(out)
        }
        // Attempt 1 was sent and billed: hand its records back with the error.
        Err(error) => Err(SectionError { error, calls }),
    }
}

/// The one repair of a result `first` (synchronous, or a batch result) that failed with
/// `errors`, and its outcome. `calls` holds only the repair call: the caller records `first`
/// (Task 16 marks a batch result `batch: true`). The rules are the module's steps 3 and 4.
pub(crate) async fn repair_once<T>(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    req: &StructuredRequest,
    label: &str,
    first: &StructuredResult,
    errors: Vec<ValidationError>,
    check: impl Fn(&StructuredResult) -> Checked<T>,
) -> Result<Attempted<T>, AiError> {
    let mut repair = req.clone();
    if !first.content.is_empty() {
        repair
            .messages
            .extend(build_repair_messages(first, &errors));
    }
    let second = call(ai, before_call, &repair).await?;
    let calls = CallRecord::from_result(&second, false);
    let (status, content, dropped) = match check(&second) {
        Checked::Clean(content) => (SectionStatus::Ready, Some(content), Vec::new()),
        Checked::Partial(content, items) => (SectionStatus::Ready, Some(content), items),
        Checked::Unusable(errors) => match check(first) {
            Checked::Partial(content, items) => (SectionStatus::Ready, Some(content), items),
            _ => (
                SectionStatus::Dropped,
                None,
                vec![DroppedItem {
                    section: label.to_owned(),
                    path: String::new(),
                    errors,
                }],
            ),
        },
    };
    Ok(Attempted {
        status,
        content,
        dropped,
        calls,
    })
}
