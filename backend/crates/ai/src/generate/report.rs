//! Report orchestration (Task 14, TS plan B3 `generateReport`, ADR 0060 "Generation").
//!
//! The five sections are five calls sharing one cached prefix. `overview` runs first, alone,
//! and is awaited: it writes the cache entry the other four read. The remaining non-skipped
//! sections then run in [`SECTION_KEYS`] order with at most `concurrency` in flight. Each
//! outcome is handed to `on_section` (awaited) as soon as it is done, so the web shows sections
//! progressively.
//!
//! On the first error (a client error such as `Unavailable` or `Budget`, or an `on_section`
//! failure) no further section is started; the sections already in flight are awaited (their
//! outcomes still reach `on_section`) and the first error is returned. The job keeps what was
//! saved and retries the rest with `skip`. The error carries every paid call that no successful
//! `on_section` received ([`ReportError::unmetered`]), so the job meters it.

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};

use super::section::{BeforeCall, CallRecord, SectionArgs, SectionOutcome, generate_section};
use crate::client::port::{AiClient, AiError};
use crate::input::types::CoachingInput;
use crate::output::codes::{SECTION_KEYS, SectionKey};
use crate::validator::evidence::build_evidence;
use crate::validator::focus::select_focuses;

/// The default number of sections in flight after `overview`.
pub const REPORT_CONCURRENCY: usize = 2;

/// Receives each finished section (the job stores, meters and announces it).
pub type OnSection<'a> =
    &'a (dyn Fn(SectionOutcome) -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync);

/// What stopped [`generate_report`].
#[derive(Debug, thiserror::Error)]
pub enum ReportCause {
    /// A model call failed or the budget hook refused it.
    #[error(transparent)]
    Ai(AiError),
    /// `on_section` failed (the section may not be stored).
    #[error("on_section failed: {0:#}")]
    OnSection(anyhow::Error),
}

/// Why [`generate_report`] stopped: the first error, plus every call that was paid for but
/// never reached a successful `on_section`, from every failed section (attempt 1 of a section
/// whose repair failed, and all calls of a section whose `on_section` failed, since
/// `on_section` is expected to store and meter atomically). The job must meter `unmetered`.
#[derive(Debug, thiserror::Error)]
#[error("{cause}")]
pub struct ReportError {
    pub cause: ReportCause,
    pub unmetered: Vec<CallRecord>,
}

impl ReportError {
    /// The client error, when that is what stopped the run.
    pub fn as_ai(&self) -> Option<&AiError> {
        match &self.cause {
            ReportCause::Ai(e) => Some(e),
            ReportCause::OnSection(_) => None,
        }
    }
}

async fn run_one(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    args: SectionArgs<'_>,
    on_section: OnSection<'_>,
) -> Result<SectionOutcome, ReportError> {
    let o = generate_section(ai, before_call, args)
        .await
        .map_err(|e| ReportError {
            cause: ReportCause::Ai(e.error),
            unmetered: e.calls,
        })?;
    if let Err(e) = on_section(o.clone()).await {
        return Err(ReportError {
            cause: ReportCause::OnSection(e),
            unmetered: o.calls,
        });
    }
    Ok(o)
}

/// Generates the report's sections not in `skip` (see the module docs). The outcomes are
/// returned in [`SECTION_KEYS`] order. A `concurrency` of 0 counts as 1.
pub async fn generate_report(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    input: &CoachingInput,
    is_first_report: bool,
    skip: &[SectionKey],
    concurrency: usize,
    on_section: OnSection<'_>,
) -> Result<Vec<SectionOutcome>, ReportError> {
    let evidence = build_evidence(input);
    let focuses = select_focuses(input, is_first_report);
    let args = |section| SectionArgs {
        input,
        evidence: &evidence,
        focuses: &focuses,
        section,
        is_first_report,
    };
    let mut outcomes = Vec::new();
    if !skip.contains(&SectionKey::Overview) {
        outcomes.push(run_one(ai, before_call, args(SectionKey::Overview), on_section).await?);
    }

    let mut queue = SECTION_KEYS
        .into_iter()
        .filter(|k| *k != SectionKey::Overview && !skip.contains(k));
    let mut running = FuturesUnordered::new();
    let mut first_error = None;
    loop {
        while first_error.is_none() && running.len() < concurrency.max(1) {
            let Some(k) = queue.next() else { break };
            running.push(run_one(ai, before_call, args(k), on_section));
        }
        match running.next().await {
            None => break,
            Some(Ok(o)) => outcomes.push(o),
            Some(Err(e)) => match &mut first_error {
                None => first_error = Some(e),
                // Only the first cause is returned, but no paid call is lost.
                Some(first) => first.unmetered.extend(e.unmetered),
            },
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    outcomes.sort_by_key(|o| o.section);
    Ok(outcomes)
}
