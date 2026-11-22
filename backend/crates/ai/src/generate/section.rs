//! Section generation (Task 14, TS plan B3 `generateSection`): one section of a report, written
//! by the model and accepted only through Task 8's validator.
//!
//! 1. **Attempt 1.** [`build_section_request`], after the budget hook. A result without parsed
//!    output becomes a validation error: `refusal` gives `Refusal`, `truncated` gives
//!    `Truncated`, anything else gives `SchemaInvalid` (one error per parse issue when the client
//!    reports them).
//! 2. **Validate.** [`validate_section`] with the report's evidence and focuses. No fatal error
//!    and no item error means `Ready` with `attempts: 1`.
//! 3. **Repair.** Otherwise one repair call ([`repair_section`]): the section request plus
//!    [`build_repair_messages`] (the raw assistant content, then the errors as
//!    `- path: code: detail`), validated the same way.
//! 4. **Drop.** After the repair, items that still fail are dropped (listed in `dropped` with
//!    their paths) and the section is `Ready` with what passed. A fatal error, an unparsable
//!    result or a refusal on attempt 2 gives `Dropped` with `content: None` and one
//!    [`DroppedItem`] for the whole section (path `""`). Text that did not pass the validator is
//!    never returned.
//!
//! Every client error (`Unavailable`, `Budget` from the hook, request errors) propagates: an
//! outage is retried by the job, never turned into a dropped section. The error is a
//! [`SectionError`] carrying the records of the calls already paid for (attempt 1 when the repair
//! fails), so they are always metered.
//!
//! A fatal or unparsable repair falls back to attempt 1's validated items when attempt 1 was only
//! partly invalid (review m1; the spec drops the bad item, not the section).

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::request::{build_repair_messages, build_section_request};
use crate::client::port::{AiClient, AiError, AiUsage, ParseError, StructuredResult};
use crate::input::types::CoachingInput;
use crate::output::codes::{DroppedItem, SectionKey, ValidationCode, ValidationError};
use crate::output::report::ValidatedSection;
use crate::output::schema::parse_report_section;
use crate::validator::evidence::Evidence;
use crate::validator::section::{SectionContext, validate_section};

/// One metered model entry of one call (a call that touched two models gives two records).
#[derive(Clone, Debug, PartialEq)]
pub struct CallRecord {
    pub model: String,
    pub usage: AiUsage,
    pub batch: bool,
    /// The SDK's client-side estimate, for comparison only, never billed (T18 controller note);
    /// on the call's first record only.
    pub sdk_cost_usd: Option<f64>,
}

impl CallRecord {
    /// One record per `model_usage` entry of `r`; the SDK estimate goes on the first.
    pub fn from_result(r: &StructuredResult, batch: bool) -> Vec<CallRecord> {
        r.model_usage
            .iter()
            .enumerate()
            .map(|(i, m)| CallRecord {
                model: m.model.clone(),
                usage: m.usage,
                batch,
                sdk_cost_usd: if i == 0 { r.sdk_cost_usd } else { None },
            })
            .collect()
    }
}

/// The budget hook, run before every model call.
pub type BeforeCall<'a> = &'a (dyn Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync);

/// Whether a section made it into the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    Ready,
    Dropped,
}

/// The result of one section. `content` is `Some` exactly when `status` is `Ready`.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionOutcome {
    pub section: SectionKey,
    pub status: SectionStatus,
    pub content: Option<ValidatedSection>,
    /// Model calls made for the section: 1 or 2.
    pub attempts: u8,
    pub dropped: Vec<DroppedItem>,
    pub calls: Vec<CallRecord>,
}

/// What one section is generated and validated against.
#[derive(Clone, Copy)]
pub struct SectionArgs<'a> {
    pub input: &'a CoachingInput,
    pub evidence: &'a Evidence,
    /// From `select_focuses`; empty means the general focus.
    pub focuses: &'a [String],
    pub section: SectionKey,
    pub is_first_report: bool,
}

/// How a result fared.
enum Checked {
    /// Validated with no error at all.
    Clean(ValidatedSection),
    /// Validated, but some items failed (they are already left out of the content).
    Partial(ValidatedSection, Vec<DroppedItem>),
    /// Nothing usable: no parsed output, or a fatal validation error.
    Unusable(Vec<ValidationError>),
}

fn verr(code: ValidationCode, path: &str, detail: impl Into<String>) -> ValidationError {
    ValidationError {
        code,
        path: path.to_owned(),
        detail: detail.into(),
    }
}

/// The validation errors of a result without parsed output.
pub(crate) fn parse_errors(r: &StructuredResult) -> Vec<ValidationError> {
    match r.parse_error {
        Some(ParseError::Refusal) => vec![verr(
            ValidationCode::Refusal,
            "",
            "the response was a refusal",
        )],
        Some(ParseError::Truncated) => vec![verr(
            ValidationCode::Truncated,
            "",
            "the response was cut off at max_tokens",
        )],
        other => match r.parse_issues.as_deref() {
            Some(issues) if !issues.is_empty() => issues
                .iter()
                .map(|i| verr(ValidationCode::SchemaInvalid, &i.path, i.code.clone()))
                .collect(),
            _ => vec![verr(
                ValidationCode::SchemaInvalid,
                "",
                format!(
                    "the response does not match the output schema ({})",
                    other.map_or("no output", ParseError::as_str)
                ),
            )],
        },
    }
}

fn check(r: &StructuredResult, args: &SectionArgs<'_>) -> Checked {
    let Some(parsed) = r.parsed.as_ref().filter(|_| r.parse_error.is_none()) else {
        return Checked::Unusable(parse_errors(r));
    };
    // The client already checked the schema; this is the typed read (and a second guard).
    let env = match parse_report_section(parsed) {
        Ok(env) => env,
        Err(issues) => {
            return Checked::Unusable(
                issues
                    .iter()
                    .map(|i| verr(ValidationCode::SchemaInvalid, &i.path, i.code.clone()))
                    .collect(),
            );
        }
    };
    let ctx = SectionContext {
        input: args.input,
        evidence: args.evidence,
        focuses: args.focuses,
    };
    let v = validate_section(args.section, &env, &ctx);
    match v.content {
        Some(content) if v.fatal.is_empty() => {
            if v.item_errors.is_empty() {
                Checked::Clean(content)
            } else {
                Checked::Partial(content, v.item_errors)
            }
        }
        _ => Checked::Unusable(v.fatal),
    }
}

pub(crate) async fn call(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    req: &crate::client::port::StructuredRequest,
) -> Result<StructuredResult, AiError> {
    before_call().await?;
    ai.structured(req).await
}

/// A section that stopped on a client error. `calls` holds the calls that were made and paid
/// for before the error (attempt 1 when the repair failed): the caller must meter them.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{error}")]
pub struct SectionError {
    pub error: AiError,
    pub calls: Vec<CallRecord>,
}

impl From<AiError> for SectionError {
    fn from(error: AiError) -> Self {
        SectionError {
            error,
            calls: Vec::new(),
        }
    }
}

/// Runs one section (see the module docs).
pub async fn generate_section(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    args: SectionArgs<'_>,
) -> Result<SectionOutcome, SectionError> {
    let req = build_section_request(args.input, args.section, args.is_first_report);
    let first = call(ai, before_call, &req).await?;
    let calls = CallRecord::from_result(&first, false);
    let errors = match check(&first, &args) {
        Checked::Clean(content) => {
            return Ok(SectionOutcome {
                section: args.section,
                status: SectionStatus::Ready,
                content: Some(content),
                attempts: 1,
                dropped: Vec::new(),
                calls,
            });
        }
        Checked::Partial(_, items) => items.into_iter().flat_map(|d| d.errors).collect(),
        Checked::Unusable(errors) => errors,
    };
    match repair_section(ai, before_call, args, first, errors).await {
        Ok(mut out) => {
            out.calls.splice(0..0, calls);
            Ok(out)
        }
        // Attempt 1 was sent and billed: hand its records back with the error.
        Err(error) => Err(SectionError { error, calls }),
    }
}

/// The one repair call for a section whose first result (`first`, synchronous or from a batch)
/// failed with `errors`, and its outcome (`attempts: 2`). `calls` holds only the repair call:
/// the caller records the first call (Task 16 marks a batch result `batch: true`).
///
/// When `first` has no content to replay (a refusal or an empty response), the repair resends
/// the section request alone: an empty assistant turn is not a valid message.
///
/// When the repair is unusable (fatal, unparsable or a refusal) but `first` validated with only
/// some items failing, the section keeps `first`'s validated items (they passed the validator;
/// its failing items stay dropped) instead of being dropped whole.
pub async fn repair_section(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    args: SectionArgs<'_>,
    first: StructuredResult,
    errors: Vec<ValidationError>,
) -> Result<SectionOutcome, AiError> {
    let mut req = build_section_request(args.input, args.section, args.is_first_report);
    if !first.content.is_empty() {
        req.messages.extend(build_repair_messages(&first, &errors));
    }
    let second = call(ai, before_call, &req).await?;
    let calls = CallRecord::from_result(&second, false);
    let (status, content, dropped) = match check(&second, &args) {
        Checked::Clean(content) => (SectionStatus::Ready, Some(content), Vec::new()),
        Checked::Partial(content, items) => (SectionStatus::Ready, Some(content), items),
        Checked::Unusable(errors) => match check(&first, &args) {
            Checked::Partial(content, items) => (SectionStatus::Ready, Some(content), items),
            _ => (
                SectionStatus::Dropped,
                None,
                vec![DroppedItem {
                    section: args.section.as_str().to_owned(),
                    path: String::new(),
                    errors,
                }],
            ),
        },
    };
    Ok(SectionOutcome {
        section: args.section,
        status,
        content,
        attempts: 2,
        dropped,
        calls,
    })
}
