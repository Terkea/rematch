//! The report payload (Task 14, TS plan B3 `assembleReportPayload`): what the web reads for one
//! report version. The JSON contract is snapshotted in
//! `backend/fixtures/ai/payload/report-payload.example.json` (`tests/computed.rs`), the hand-off
//! file for the web.
//!
//! Status: `complete` when all five sections are ready, `computed_only` when none is, `partial`
//! otherwise. `aiDisclosure` is present exactly when some section is ready. A second snapshot,
//! `report-payload.partial.example.json`, shows a partial payload: a null (dropped) section,
//! absent sections and a non-empty `dropped`.
//!
//! Contract notes for the web:
//! - `dropped` never carries model text. Each entry is the section, the JSON path of the dropped
//!   part and, per error, the stable [`ValidationCode`] and its path (review I2). The `detail`
//!   strings, which quote the rejected fragment, stay server-side in `report_sections.dropped`.
//! - A `dropped` path indexes the model's output (`weaknesses.items[1]` is the model's second
//!   item), not the kept array, which is renumbered (review m2). Path `""` is the whole section.
//! - Kept items are in the model's order. Show weaknesses ranked by sorting on `weaknessId`
//!   against `computed.weaknesses[].rank`, and openings by `lineKey` against
//!   `computed.openings` (review m3).

use std::collections::BTreeMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::computed::ComputedReport;
use super::section::{SectionOutcome, SectionStatus};
pub use crate::db::reports::ReportStatus;
use crate::db::reports::{SectionWriteStatus, load_report_rows};
use crate::output::codes::{DroppedItem, SECTION_KEYS, SectionKey, ValidationCode};
use crate::output::disclosure::AiDisclosure;
use crate::output::report::ValidatedSection;

/// `ReportPayload.schemaVersion` (also `reports.schema_version`).
pub const REPORT_PAYLOAD_SCHEMA_VERSION: &str = "report-payload-v1";

/// TS `ReportPayload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportPayload {
    pub schema_version: String,
    pub report_id: Uuid,
    pub version: i32,
    pub status: ReportStatus,
    pub computed: ComputedReport,
    /// Keyed by the snake_case section key. A dropped section is `null`; a section not
    /// generated (yet) is absent.
    pub sections: BTreeMap<SectionKey, Option<ValidatedSection>>,
    /// Every dropped item and dropped section, in section order, without model text.
    pub dropped: Vec<PayloadDroppedItem>,
    pub ai_disclosure: Option<AiDisclosure>,
    /// `Date.toISOString()` form.
    pub generated_at: String,
}

/// A [`DroppedItem`] as the web sees it: no `detail`, so no rejected model text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayloadDroppedItem {
    pub section: String,
    pub path: String,
    pub errors: Vec<PayloadDroppedError>,
}

/// One validation error of a dropped part: the stable code and our own JSON path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayloadDroppedError {
    pub code: ValidationCode,
    pub path: String,
}

impl From<&DroppedItem> for PayloadDroppedItem {
    fn from(d: &DroppedItem) -> Self {
        PayloadDroppedItem {
            section: d.section.clone(),
            path: d.path.clone(),
            errors: d
                .errors
                .iter()
                .map(|e| PayloadDroppedError {
                    code: e.code,
                    path: e.path.clone(),
                })
                .collect(),
        }
    }
}

/// One section as the payload sees it. `dropped` keeps the full server-side items; the
/// payload projects them to [`PayloadDroppedItem`].
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadSection {
    pub section: SectionKey,
    pub status: SectionStatus,
    pub content: Option<ValidatedSection>,
    pub dropped: Vec<DroppedItem>,
}

impl From<&SectionOutcome> for PayloadSection {
    fn from(o: &SectionOutcome) -> Self {
        PayloadSection {
            section: o.section,
            status: o.status,
            content: o.content.clone(),
            dropped: o.dropped.clone(),
        }
    }
}

/// The inputs of [`assemble_report_payload`].
#[derive(Clone, Debug, PartialEq)]
pub struct PayloadParts {
    pub report_id: Uuid,
    pub version: i32,
    pub computed: ComputedReport,
    pub sections: Vec<PayloadSection>,
    pub ai_disclosure: Option<AiDisclosure>,
    pub generated_at: OffsetDateTime,
}

/// The report status for these sections (one entry per section; a later entry wins).
pub fn report_status(sections: &[PayloadSection]) -> ReportStatus {
    let by_key: BTreeMap<SectionKey, &PayloadSection> =
        sections.iter().map(|s| (s.section, s)).collect();
    status_of(&by_key)
}

fn status_of(by_key: &BTreeMap<SectionKey, &PayloadSection>) -> ReportStatus {
    let ready = by_key
        .values()
        .filter(|s| s.status == SectionStatus::Ready && s.content.is_some())
        .count();
    if ready == SECTION_KEYS.len() {
        ReportStatus::Complete
    } else if ready == 0 {
        ReportStatus::ComputedOnly
    } else {
        ReportStatus::Partial
    }
}

/// Builds the payload (see the module docs). A ready section shows its content; a dropped one
/// shows `null`. The disclosure is kept only when some section is ready.
pub fn assemble_report_payload(parts: PayloadParts) -> ReportPayload {
    let by_key: BTreeMap<SectionKey, &PayloadSection> =
        parts.sections.iter().map(|s| (s.section, s)).collect();
    let status = status_of(&by_key);
    let sections = by_key
        .iter()
        .map(|(k, s)| {
            let content = match s.status {
                SectionStatus::Ready => s.content.clone(),
                SectionStatus::Dropped => None,
            };
            (*k, content)
        })
        .collect();
    let dropped = by_key
        .values()
        .flat_map(|s| s.dropped.iter().map(PayloadDroppedItem::from))
        .collect();
    ReportPayload {
        schema_version: REPORT_PAYLOAD_SCHEMA_VERSION.to_owned(),
        report_id: parts.report_id,
        version: parts.version,
        status,
        computed: parts.computed,
        sections,
        dropped,
        ai_disclosure: match status {
            ReportStatus::ComputedOnly => None,
            _ => parts.ai_disclosure,
        },
        generated_at: common::js_time::to_iso_string(parts.generated_at),
    }
}

/// The stored report as a payload (`None` when there is no such report), assembled from
/// `reports::load_report_rows` with the same rules; `generatedAt` is the report's
/// `updated_at`. Stored JSON that does not read back, or a row of another payload schema
/// version, is an error naming the column.
pub async fn load_report_payload(
    conn: &mut PgConnection,
    report_id: Uuid,
) -> anyhow::Result<Option<ReportPayload>> {
    let Some(stored) = load_report_rows(conn, report_id).await? else {
        return Ok(None);
    };
    let r = stored.report;
    if r.schema_version != REPORT_PAYLOAD_SCHEMA_VERSION {
        anyhow::bail!(
            "reports.schema_version is {:?}, expected {REPORT_PAYLOAD_SCHEMA_VERSION:?}",
            r.schema_version
        );
    }
    let computed: ComputedReport =
        serde_json::from_value(r.computed).context("reports.computed")?;
    let ai_disclosure = r
        .ai_disclosure
        .map(serde_json::from_value::<AiDisclosure>)
        .transpose()
        .context("reports.ai_disclosure")?;
    let mut sections = Vec::with_capacity(stored.sections.len());
    for s in stored.sections {
        let what = format!("report_sections {}", s.section);
        let content = s
            .content
            .map(serde_json::from_value::<ValidatedSection>)
            .transpose()
            .with_context(|| format!("{what}.content"))?;
        let dropped: Vec<DroppedItem> =
            serde_json::from_value(s.dropped).with_context(|| format!("{what}.dropped"))?;
        sections.push(PayloadSection {
            section: s.section,
            status: match s.status {
                SectionWriteStatus::Ready => SectionStatus::Ready,
                SectionWriteStatus::Dropped => SectionStatus::Dropped,
            },
            content,
            dropped,
        });
    }
    Ok(Some(assemble_report_payload(PayloadParts {
        report_id: r.id,
        version: r.version,
        computed,
        sections,
        ai_disclosure,
        generated_at: r.updated_at,
    })))
}
