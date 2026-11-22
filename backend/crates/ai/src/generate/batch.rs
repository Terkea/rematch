//! Batch request building and result processing (Task 16, ADR 0062: batches go through the
//! Messages Batches client with the same `MessagesRequest::structured` wire as a sync call).
//!
//! **Building.** Each batch item is the synchronous request of its generator
//! ([`build_section_request`], [`build_weekly_request`], [`build_change_request`]) under a
//! custom id `<schema.id>__v<schema.version>__<item id as simple hex>` ([`batch_custom_id`]),
//! which the results map back to the schema ([`RegistryResolver`]) and to the item
//! ([`batch_item_id`]) in any process. [`chunk_batch`] splits a submission so no batch holds more
//! than [`MAX_BATCH_REQUESTS`] requests or [`MAX_BATCH_BYTES`] of create body.
//!
//! **Processing.** A batch result goes through exactly the validation and repair-once rules of
//! the interactive path, so nothing unvalidated is stored:
//!
//! - succeeded and valid: ready, with no sync call;
//! - succeeded but invalid (item or fatal validation errors), or a `refusal` / `truncated`
//!   answer: exactly one synchronous repair through the generator's standalone repair entry
//!   ([`repair_section`], [`repair_weekly_review`], [`repair_weakness_change`]);
//! - any other parse error (`no_schema`, `no_text`, `invalid_json`, `schema_mismatch`), a custom
//!   id of another schema, or an `errored`, `canceled` or `expired` outcome: `RetrySync`, and the
//!   caller runs the generator synchronously from scratch.
//!
//! **Metering.** Every result that carries usage was billed (at batch rates), so its records
//! are returned with `batch: true` whatever happens next: in the outcome's `calls` (first), in
//! `RetrySync { calls }`, or in the [`SectionError`] of a failed repair. `errored`, `canceled`
//! and `expired` results carry no message and no usage, and the API does not bill them, so their
//! `RetrySync` has no record.

use std::io;

use serde::Serialize;
use uuid::Uuid;

use super::attempt::{Attempted, Checked};
use super::change::{
    ChangeOutcome, build_change_evidence, build_change_request, change_outcome, change_text_rules,
    repair_weakness_change,
};
use super::request::build_section_request;
use super::section::{
    BeforeCall, CallRecord, SectionArgs, SectionError, SectionOutcome, SectionStatus,
    check_section, repair_section,
};
use super::weekly::{
    WeeklyOutcome, build_weekly_evidence, build_weekly_request, repair_weekly_review,
    weekly_outcome,
};
use crate::client::custom_id::{CustomIdParts, decode_custom_id, encode_custom_id};
use crate::client::port::{
    AiClient, BatchItem, BatchOutcome, BatchResultItem, OutputSchema, ParseError, SchemaResolver,
    StructuredResult,
};
use crate::client::wire::MessagesRequest;
use crate::input::types::CoachingInput;
use crate::output::change::WeaknessChangeInput;
use crate::output::codes::{SectionKey, ValidationError};
use crate::output::schema::{
    REPORT_SECTION_SCHEMA, WEAKNESS_CHANGE_SCHEMA, WEEKLY_REVIEW_SCHEMA, schema_by_id,
};
use crate::output::weekly::WeeklyReviewInput;
use crate::validator::evidence::build_evidence;
use crate::validator::focus::select_focuses;

/// The most requests in one batch (the API's limit).
pub const MAX_BATCH_REQUESTS: usize = 10_000;

/// The largest create body of one batch: 200 MiB, below the API's 256 MB.
pub const MAX_BATCH_BYTES: usize = 200 * 1024 * 1024;

/// `<schema.id>__v<schema.version>__<item_id as simple hex>`: at most 53 characters for the
/// registered schemas (52 for `report-section`), within the API's `[A-Za-z0-9_-]{1,64}`.
pub fn batch_custom_id(schema: &OutputSchema, item_id: Uuid) -> String {
    encode_custom_id(&CustomIdParts {
        schema_id: schema.id.to_owned(),
        version: schema.version,
        reference: item_id.simple().to_string(),
    })
    .unwrap_or_else(|e| panic!("schema {:?}: {e}", schema.id))
}

/// The item id of a custom id made by [`batch_custom_id`] (whatever its schema or version), or
/// `None` for a foreign id.
pub fn batch_item_id(custom_id: &str) -> Option<Uuid> {
    let parts = decode_custom_id(custom_id)?;
    let r = &parts.reference;
    // Only the simple form we encode: 32 lowercase hex digits.
    if r.len() != 32 || !r.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    Uuid::try_parse(r).ok()
}

/// Resolves a custom id to its registered schema (exact id and version), in any process.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegistryResolver;

impl SchemaResolver for RegistryResolver {
    fn resolve(&self, custom_id: &str) -> Option<&'static OutputSchema> {
        let parts = decode_custom_id(custom_id)?;
        schema_by_id(&parts.schema_id, parts.version)
    }
}

/// The batch item of one report section: exactly the synchronous [`build_section_request`].
pub fn section_batch_item(
    input: &CoachingInput,
    section: SectionKey,
    is_first_report: bool,
    item_id: Uuid,
) -> BatchItem {
    BatchItem {
        custom_id: batch_custom_id(&REPORT_SECTION_SCHEMA, item_id),
        req: build_section_request(input, section, is_first_report),
    }
}

/// The batch item of a weekly review: exactly the synchronous [`build_weekly_request`].
pub fn weekly_batch_item(input: &WeeklyReviewInput, item_id: Uuid) -> BatchItem {
    BatchItem {
        custom_id: batch_custom_id(&WEEKLY_REVIEW_SCHEMA, item_id),
        req: build_weekly_request(input),
    }
}

/// The batch item of a change note: exactly the synchronous [`build_change_request`].
pub fn change_batch_item(input: &WeaknessChangeInput, item_id: Uuid) -> BatchItem {
    BatchItem {
        custom_id: batch_custom_id(&WEAKNESS_CHANGE_SCHEMA, item_id),
        req: build_change_request(input),
    }
}

/// Counts bytes without keeping them.
struct Count(usize);

impl io::Write for Count {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize)]
struct Line<'a> {
    custom_id: &'a str,
    params: MessagesRequest,
}

/// The serialised size of one `{"custom_id":…,"params":…}` entry of the create body.
fn item_bytes(item: &BatchItem) -> usize {
    let mut n = Count(0);
    serde_json::to_writer(
        &mut n,
        &Line {
            custom_id: &item.custom_id,
            params: MessagesRequest::structured(&item.req),
        },
    )
    .expect("a batch request serialises");
    n.0
}

/// `{"requests":[]}`.
const BODY_FRAME: usize = 15;

/// The exact size of the create body (`{"requests":[…]}`) of `items`.
pub fn batch_request_bytes(items: &[BatchItem]) -> usize {
    BODY_FRAME + items.iter().map(item_bytes).sum::<usize>() + items.len().saturating_sub(1)
}

/// Splits `items` into batches of at most [`MAX_BATCH_REQUESTS`] requests and
/// [`MAX_BATCH_BYTES`] of create body, in order.
pub fn chunk_batch(items: Vec<BatchItem>) -> Vec<Vec<BatchItem>> {
    chunk_batch_by(items, MAX_BATCH_REQUESTS, MAX_BATCH_BYTES)
}

/// [`chunk_batch`] with explicit limits (a `max_requests` of 0 counts as 1). Greedy and
/// order-preserving: a batch is closed when the next item would cross either limit. An item too
/// large for `max_bytes` on its own goes alone into its own batch (the API rejects it then; it
/// is never dropped silently).
pub fn chunk_batch_by(
    items: Vec<BatchItem>,
    max_requests: usize,
    max_bytes: usize,
) -> Vec<Vec<BatchItem>> {
    let max_requests = max_requests.max(1);
    let mut chunks = Vec::new();
    let mut current: Vec<BatchItem> = Vec::new();
    let mut bytes = BODY_FRAME;
    for item in items {
        let size = item_bytes(&item);
        let added = size + usize::from(!current.is_empty());
        if !current.is_empty()
            && (current.len() >= max_requests || bytes.saturating_add(added) > max_bytes)
        {
            chunks.push(std::mem::take(&mut current));
            bytes = BODY_FRAME;
        }
        bytes += size + usize::from(!current.is_empty());
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// What became of one report-section batch result.
#[derive(Clone, Debug, PartialEq)]
pub enum BatchProcessed {
    /// The section, validated (and repaired once when needed). `calls[0]` is the batch result
    /// (`batch: true`); a repair adds its sync call.
    Outcome(Box<SectionOutcome>),
    /// Unusable: run the section synchronously. `calls` holds the billed batch result, if any;
    /// meter it.
    RetrySync { calls: Vec<CallRecord> },
}

/// What became of one weekly-review batch result (see [`BatchProcessed`]).
#[derive(Clone, Debug, PartialEq)]
pub enum WeeklyProcessed {
    Outcome(Box<WeeklyOutcome>),
    RetrySync { calls: Vec<CallRecord> },
}

/// What became of one change-note batch result (see [`BatchProcessed`]).
#[derive(Clone, Debug, PartialEq)]
pub enum ChangeProcessed {
    Outcome(Box<ChangeOutcome>),
    RetrySync { calls: Vec<CallRecord> },
}

/// The first look at a batch result.
enum Triage<T> {
    Ready(T),
    Repair(Box<StructuredResult>, Vec<ValidationError>),
    Retry,
}

/// Sorts `r` (see the module docs) and returns the billed records of its batch call.
fn triage<T>(
    r: BatchResultItem,
    expected: &OutputSchema,
    check: impl Fn(&StructuredResult) -> Checked<T>,
) -> (Triage<T>, Vec<CallRecord>) {
    let Some(result) = r.result.filter(|_| r.outcome == BatchOutcome::Succeeded) else {
        return (Triage::Retry, Vec::new());
    };
    let calls = CallRecord::from_result(&result, true);
    // The result must belong to the schema the caller validates against.
    let ours = decode_custom_id(&r.custom_id)
        .is_some_and(|p| p.schema_id == expected.id && p.version == expected.version);
    if !ours {
        return (Triage::Retry, calls);
    }
    match result.parse_error {
        None | Some(ParseError::Refusal | ParseError::Truncated) => {}
        Some(_) => return (Triage::Retry, calls),
    }
    let triage = match check(&result) {
        Checked::Clean(content) => Triage::Ready(content),
        Checked::Partial(_, items) => Triage::Repair(
            Box::new(result),
            items.into_iter().flat_map(|d| d.errors).collect(),
        ),
        Checked::Unusable(errors) => Triage::Repair(Box::new(result), errors),
    };
    (triage, calls)
}

/// The batch call's records first, then the repair's.
fn prepend(mut calls: Vec<CallRecord>, batch: &[CallRecord]) -> Vec<CallRecord> {
    calls.splice(0..0, batch.iter().cloned());
    calls
}

/// Processes one report-section batch result (see the module docs). A client error of the
/// repair (`Unavailable`, `Budget` from the hook) is a [`SectionError`] whose `calls` hold the
/// billed batch result, so it is always metered.
pub async fn process_report_result(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    r: BatchResultItem,
    input: &CoachingInput,
    section: SectionKey,
    is_first_report: bool,
) -> Result<BatchProcessed, SectionError> {
    let evidence = build_evidence(input);
    let focuses = select_focuses(input, is_first_report);
    let args = SectionArgs {
        input,
        evidence: &evidence,
        focuses: &focuses,
        section,
        is_first_report,
    };
    let (triage, calls) = triage(r, &REPORT_SECTION_SCHEMA, |res| check_section(res, &args));
    match triage {
        Triage::Retry => Ok(BatchProcessed::RetrySync { calls }),
        Triage::Ready(content) => Ok(BatchProcessed::Outcome(Box::new(SectionOutcome {
            section,
            status: SectionStatus::Ready,
            content: Some(content),
            attempts: 1,
            dropped: Vec::new(),
            calls,
        }))),
        Triage::Repair(first, errors) => {
            match repair_section(ai, before_call, args, *first, errors).await {
                Ok(mut o) => {
                    o.calls = prepend(o.calls, &calls);
                    Ok(BatchProcessed::Outcome(Box::new(o)))
                }
                Err(error) => Err(SectionError { error, calls }),
            }
        }
    }
}

/// Processes one weekly-review batch result, with the same rules as
/// [`process_report_result`].
pub async fn process_weekly_result(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    r: BatchResultItem,
    input: &WeeklyReviewInput,
) -> Result<WeeklyProcessed, SectionError> {
    let ev = build_weekly_evidence(input);
    let (triage, calls) = triage(r, &WEEKLY_REVIEW_SCHEMA, |res| {
        super::weekly::check(res, input, &ev)
    });
    match triage {
        Triage::Retry => Ok(WeeklyProcessed::RetrySync { calls }),
        Triage::Ready(content) => Ok(WeeklyProcessed::Outcome(Box::new(weekly_outcome(
            Attempted {
                status: SectionStatus::Ready,
                content: Some(content),
                dropped: Vec::new(),
                calls,
            },
        )))),
        Triage::Repair(first, errors) => {
            match repair_weekly_review(ai, before_call, input, &first, errors).await {
                Ok(mut o) => {
                    o.calls = prepend(o.calls, &calls);
                    Ok(WeeklyProcessed::Outcome(Box::new(o)))
                }
                Err(error) => Err(SectionError { error, calls }),
            }
        }
    }
}

/// Processes one change-note batch result, with the same rules as [`process_report_result`].
pub async fn process_change_result(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    r: BatchResultItem,
    input: &WeaknessChangeInput,
) -> Result<ChangeProcessed, SectionError> {
    let ev = build_change_evidence(input);
    let rules = change_text_rules(input);
    let (triage, calls) = triage(r, &WEAKNESS_CHANGE_SCHEMA, |res| {
        super::change::check(res, &rules, &ev)
    });
    match triage {
        Triage::Retry => Ok(ChangeProcessed::RetrySync { calls }),
        Triage::Ready(note) => Ok(ChangeProcessed::Outcome(Box::new(change_outcome(
            Attempted {
                status: SectionStatus::Ready,
                content: Some(note),
                dropped: Vec::new(),
                calls,
            },
        )))),
        Triage::Repair(first, errors) => {
            match repair_weakness_change(ai, before_call, input, &first, errors).await {
                Ok(mut o) => {
                    o.calls = prepend(o.calls, &calls);
                    Ok(ChangeProcessed::Outcome(Box::new(o)))
                }
                Err(error) => Err(SectionError { error, calls }),
            }
        }
    }
}
