//! ADR 0094: the coach knows the player's plan. A per-user context block (the running weekly
//! focus of ADR 0091, the trainer's last 7 and 30 days and its "Is it working?" verdicts of ADR
//! 0090) built when a chat turn or a weekly review is written, next to the per-account coaching
//! input. It is never stored in the account's snapshot (ADR 0086), so one user's plan never
//! reaches another user linked to the same account.

pub mod compute;
pub mod db;
pub mod types;

use serde_json::{Value, json};

pub use db::{load_plan, verified_owner};
pub use types::*;

use crate::generate::context::canonical_json_escaped;
use crate::input::types::CoachingInput;
use crate::validator::evidence::{Evidence, create_evidence, list_date_paths, list_number_paths};

/// The plan for an account-level text (the weekly review): the plan of the account's verified
/// owner (ADR-FIX-N-1: the one reader of its AI artefacts), else none. A database error is
/// logged and gives none: the review is written without the plan.
pub async fn owner_plan(
    pool: &sqlx::PgPool,
    account_id: uuid::Uuid,
    tc: crate::input::types::TimeClass,
    now: time::OffsetDateTime,
    label_of: &(dyn Fn(&str) -> Option<String> + Sync),
) -> Option<PlanContext> {
    let r = async {
        let mut conn = pool.acquire().await?;
        let Some(owner) = verified_owner(&mut conn, account_id).await? else {
            return Ok(None);
        };
        load_plan(&mut conn, owner, account_id, tc, now, label_of).await
    }
    .await;
    r.unwrap_or_else(|e: sqlx::Error| {
        tracing::warn!(error = %e, "loading the owner's plan failed");
        None
    })
}

/// The plan as JSON (what the evidence and the prompt read).
pub fn plan_json(plan: &PlanContext) -> Value {
    serde_json::to_value(plan).expect("PlanContext serialises")
}

/// The plan's citable paths alone: `plan.…` numbers and dates.
fn plan_refs(plan: &PlanContext) -> Value {
    let mut ev = create_evidence();
    ev.add_plan(&plan_json(plan));
    json!({ "num": list_number_paths(&ev), "date": list_date_paths(&ev) })
}

/// What the model reads: the plan plus `refs`, the `{{num:plan.…}}` and `{{date:plan.…}}`
/// paths it may cite.
pub fn plan_view(plan: &PlanContext) -> Value {
    let mut v = plan_json(plan);
    v["refs"] = plan_refs(plan);
    v
}

/// `"<plan_context>" + canonical_json(plan_view(plan)) + "</plan_context>"`, `<` escaped.
pub fn plan_block(plan: &PlanContext) -> String {
    format!(
        "<plan_context>{}</plan_context>",
        canonical_json_escaped(&plan_view(plan))
    )
}

/// Adds the plan to a turn's evidence (its paths and numbers become citable).
pub fn add_plan_evidence(ev: &mut Evidence, plan: &PlanContext) {
    ev.add_plan(&plan_json(plan));
}

/// A series' plain name from the coaching input, for the series the catalogue does not name:
/// a colour row, a then-vs-now row or a timeline change with that series key, or an opening
/// line (`opening:<colour>:<lineKey>`).
pub fn label_from_input(input: &CoachingInput, series_key: &str) -> Option<String> {
    let json = input.to_json();
    let rows = [
        &json["byColor"]["white"]["weaknesses"],
        &json["byColor"]["black"]["weaknesses"],
        &json["trajectory"]["thenVsNow"]["weaknesses"],
        &json["changes"],
    ];
    for r in rows {
        if let Some(l) = r.as_array().into_iter().flatten().find_map(|w| {
            (w["seriesKey"].as_str() == Some(series_key))
                .then(|| w["label"].as_str().map(str::to_owned))
                .flatten()
        }) {
            return Some(l);
        }
    }
    let rest = series_key.strip_prefix("opening:")?;
    let (colour, line) = rest.split_once(':')?;
    let o = input.openings.iter().find(|o| o.line_key == line)?;
    let side = if colour == "white" { "White" } else { "Black" };
    Some(format!("{} (as {side})", o.name))
}
