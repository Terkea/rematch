//! The weekly review (Task 15, TS plan B4 `generateWeeklyReview`), on the lighter model.
//!
//! The request shares the report's first system block ([`rules_block`]), then the weekly prompt
//! and its own JSON output instructions (`weekly-review/output`: the weekly envelope is not a
//! report section, so `report/output` does not fit), then `<weekly_input>` with the canonical
//! context (cached) and the uncached task.
//!
//! Every item is validated with [`validate_text`] and the rules of [`weekly_text_rules`], and
//! the result goes through Task 14's repair-once flow ([`super::attempt`]). Nothing that failed
//! the validator is returned.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::attempt::{Attempted, Checked, attempt_and_repair, parsed};
use super::context::canonical_json_escaped;
use super::request::rules_block;
use super::section::{BeforeCall, CallRecord, SectionError};
use crate::client::model::{Effort, Models};
use crate::client::port::{
    AiClient, Message, Role, StructuredRequest, StructuredResult, TextBlock,
};
use crate::output::codes::{DroppedItem, ValidationCode, ValidationError};
use crate::output::report::{Item, ValidatedItem};
use crate::output::schema::{WEEKLY_REVIEW_SCHEMA, parse_weekly_review};
use crate::output::weekly::{ValidatedWeeklyReview, WeeklyReviewInput, WeeklyReviewOutput};
use crate::prompts::load_prompt;
use crate::validator::evidence::{
    Evidence, build_evidence_from, list_date_paths, list_number_paths, path_segment,
};
use crate::validator::language::{
    banned_phrases, engine_claims, likelihood, likelihood_phrase, no_causal_phrases, no_marketing,
    no_quantity_words,
};
use crate::validator::text::{TextCtx, TextRule, TextRules, validate_text};
use crate::validator::tokens::TokenBody;

/// `max_tokens` of a weekly review call.
pub const WEEKLY_MAX_TOKENS: u32 = 4000;

/// `DroppedItem::section` of the weekly review.
pub const WEEKLY_SECTION: &str = "weekly";

/// The detail of a process text that quotes a `metricDeltas` number (ADR-R3-4: a reuse of
/// `likelihood_mismatch`, since an outcome number in the process text claims an outcome).
pub const PROCESS_OUTCOME_NUMBER: &str = "process quotes an outcome number";

fn text(id: &str) -> &'static str {
    load_prompt(id, 1).unwrap_or_else(|e| panic!("{e}")).text
}

fn input_json(input: &WeeklyReviewInput) -> Value {
    serde_json::to_value(input).expect("WeeklyReviewInput serialises")
}

/// The index of a weekly review: the week's exemplars, its changes (with their examples, ratios
/// and rates) and every number and date path of the weekly input. `focusUpdates.<id>.pImp` is a
/// rate. Positions outside the week (other weaknesses' exemplars) are not in it.
pub fn build_weekly_evidence(input: &WeeklyReviewInput) -> Evidence {
    let exemplars: Vec<_> = input.exemplars.iter().collect();
    let changes: Vec<_> = input.changes.iter().collect();
    let rates: Vec<String> = input
        .focus_updates
        .iter()
        .map(|f| format!("focusUpdates.{}.pImp", path_segment(&f.weakness_id)))
        .collect();
    build_evidence_from(&input_json(input), &exemplars, &changes, &rates)
}

/// What the weekly prompts read: the input, plus `likelihoodPhrase` on each focus (null without
/// `pImp`) and `refs` (the `num` and `date` token paths, sorted).
pub fn build_weekly_context(input: &WeeklyReviewInput) -> Value {
    let mut ctx = input_json(input);
    if let Some(Value::Array(fs)) = ctx.get_mut("focusUpdates") {
        for (f, v) in input.focus_updates.iter().zip(fs.iter_mut()) {
            if let Value::Object(o) = v {
                o.insert("likelihoodPhrase".into(), json!(likelihood_phrase(f.p_imp)));
            }
        }
    }
    let ev = build_weekly_evidence(input);
    ctx["refs"] = json!({
        "num": list_number_paths(&ev),
        "date": list_date_paths(&ev),
    });
    ctx
}

/// The weekly review request: `Models::WEEKLY`, low effort, [`WEEKLY_MAX_TOKENS`].
/// - `system[0]`: [`rules_block`] (cached, shared with reports and the chat);
/// - `system[1]`: `weekly-review/system` + `weekly-review/output` (cached);
/// - user: `<weekly_input>` + the canonical context (cached), then `weekly-review/task`.
pub fn build_weekly_request(input: &WeeklyReviewInput) -> StructuredRequest {
    StructuredRequest {
        model: Models::WEEKLY,
        system: vec![
            rules_block(),
            TextBlock::cached(format!(
                "{}\n\n{}",
                text("weekly-review/system"),
                text("weekly-review/output")
            )),
        ],
        messages: vec![Message {
            role: Role::User,
            content: vec![
                TextBlock::cached(format!(
                    "<weekly_input>{}</weekly_input>",
                    canonical_json_escaped(&build_weekly_context(input))
                ))
                .to_json(),
                TextBlock::new(text("weekly-review/task")).to_json(),
            ],
        }],
        max_tokens: WEEKLY_MAX_TOKENS,
        effort: Effort::Low,
        schema: &WEEKLY_REVIEW_SCHEMA,
    }
}

/// Which item of the review a rule set is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeeklyItemKind {
    Headline,
    Process,
    /// `outcomes[i]`, about `focusUpdates[i]`.
    Outcome(usize),
    Exemplar,
    NextWeek,
}

/// `process` may not quote a number under `metricDeltas` (`likelihood_mismatch`).
fn no_outcome_numbers() -> TextRule {
    Box::new(|_: &str, ctx: &TextCtx| {
        let quoted = ctx.tokens.iter().any(|t| {
            matches!(&t.body, TokenBody::Num { path, .. }
                if path == "metricDeltas" || path.starts_with("metricDeltas."))
        });
        if quoted {
            vec![(
                ValidationCode::LikelihoodMismatch,
                PROCESS_OUTCOME_NUMBER.to_owned(),
            )]
        } else {
            Vec::new()
        }
    })
}

/// The rules of one weekly item (ADR-R3-4 "Weekly review and change notes"):
/// - every item: `banned_phrases` and `no_marketing` (TS B4), plus the report's `Item` rules
///   (`engine_claims`, `no_quantity_words`, `no_causal_phrases`);
/// - `outcomes[i]`: `likelihood` with the phrase of `focusUpdates[i].pImp` (none past the last
///   focus, so any likelihood or direction word is a mismatch);
/// - every other item: `likelihood(None)`, since outcome claims belong in the outcomes;
/// - `process`: no `metricDeltas` number;
/// - an exemplar item: a citation (tied to the week's exemplars by [`validate_weekly`]).
pub fn weekly_text_rules(input: &WeeklyReviewInput, kind: WeeklyItemKind) -> TextRules {
    let mut rules = TextRules {
        extra: vec![
            banned_phrases(),
            no_marketing(),
            engine_claims(),
            no_quantity_words(),
            no_causal_phrases(),
        ],
        ..TextRules::default()
    };
    match kind {
        WeeklyItemKind::Outcome(i) => {
            let p = input.focus_updates.get(i).and_then(|f| f.p_imp);
            rules.extra.push(likelihood(likelihood_phrase(p)));
        }
        WeeklyItemKind::Process => {
            rules.extra.push(likelihood(None));
            rules.extra.push(no_outcome_numbers());
        }
        WeeklyItemKind::Exemplar => {
            rules.require_citation = true;
            rules.extra.push(likelihood(None));
        }
        WeeklyItemKind::Headline | WeeklyItemKind::NextWeek => {
            rules.extra.push(likelihood(None));
        }
    }
    rules
}

struct V<'a> {
    input: &'a WeeklyReviewInput,
    ev: &'a Evidence,
    exemplars: HashSet<String>,
    dropped: Vec<DroppedItem>,
}

impl V<'_> {
    fn item(&mut self, it: &Item, kind: WeeklyItemKind, path: &str) -> Option<ValidatedItem> {
        let rules = weekly_text_rules(self.input, kind);
        let result = validate_text(
            &it.text,
            &it.citations,
            self.ev,
            &rules,
            &format!("{path}.text"),
        );
        let mut unrelated = Vec::new();
        if kind == WeeklyItemKind::Exemplar {
            for (i, c) in it.citations.iter().enumerate() {
                let k = format!("{}#{}", c.game_id, c.ply);
                if self.ev.positions.contains_key(&k) && !self.exemplars.contains(&k) {
                    unrelated.push(ValidationError {
                        code: ValidationCode::UnrelatedCitation,
                        path: format!("{path}.citations[{i}]"),
                        detail: format!("{k} is not one of the week's exemplars"),
                    });
                }
            }
        }
        let errors = match result {
            Ok(segments) if unrelated.is_empty() => {
                return Some(ValidatedItem {
                    text: it.text.clone(),
                    citations: it.citations.clone(),
                    segments,
                });
            }
            Ok(_) => unrelated,
            Err(mut e) => {
                e.extend(unrelated);
                e
            }
        };
        self.dropped.push(DroppedItem {
            section: WEEKLY_SECTION.to_owned(),
            path: path.to_owned(),
            errors,
        });
        None
    }
}

/// Validates a weekly review item by item. A failing singleton becomes `None` and a failing
/// entry is removed; each is listed with its path (`headline`, `outcomes[0]`, `nextWeek`), its
/// errors at `<path>.text`.
pub fn validate_weekly(
    out: &WeeklyReviewOutput,
    input: &WeeklyReviewInput,
    ev: &Evidence,
) -> (ValidatedWeeklyReview, Vec<DroppedItem>) {
    let mut v = V {
        input,
        ev,
        exemplars: input
            .exemplars
            .iter()
            .map(|e| format!("{}#{}", e.game_id, e.ply))
            .collect(),
        dropped: Vec::new(),
    };
    let headline = v.item(&out.headline, WeeklyItemKind::Headline, "headline");
    let process = v.item(&out.process, WeeklyItemKind::Process, "process");
    let outcomes = out
        .outcomes
        .iter()
        .enumerate()
        .filter_map(|(i, it)| v.item(it, WeeklyItemKind::Outcome(i), &format!("outcomes[{i}]")))
        .collect();
    let exemplars = out
        .exemplars
        .iter()
        .enumerate()
        .filter_map(|(i, it)| v.item(it, WeeklyItemKind::Exemplar, &format!("exemplars[{i}]")))
        .collect();
    let next_week = v.item(&out.next_week, WeeklyItemKind::NextWeek, "nextWeek");
    (
        ValidatedWeeklyReview {
            headline,
            process,
            outcomes,
            exemplars,
            next_week,
        },
        v.dropped,
    )
}

/// The result of a weekly review. When nothing usable came back after the repair, `content` is
/// empty (every part `None` or `[]`) and `dropped` holds one entry at path `""`.
#[derive(Clone, Debug, PartialEq)]
pub struct WeeklyOutcome {
    pub content: ValidatedWeeklyReview,
    pub dropped: Vec<DroppedItem>,
    /// Every model call made: 1 or 2. All of them are metered.
    pub calls: Vec<CallRecord>,
}

fn empty_review() -> ValidatedWeeklyReview {
    ValidatedWeeklyReview {
        headline: None,
        process: None,
        outcomes: Vec::new(),
        exemplars: Vec::new(),
        next_week: None,
    }
}

fn check(
    r: &StructuredResult,
    input: &WeeklyReviewInput,
    ev: &Evidence,
) -> Checked<ValidatedWeeklyReview> {
    let value = match parsed(r) {
        Ok(v) => v,
        Err(errors) => return Checked::Unusable(errors),
    };
    let out = match parse_weekly_review(value) {
        Ok(out) => out,
        Err(issues) => {
            return Checked::Unusable(
                issues
                    .into_iter()
                    .map(|i| ValidationError {
                        code: ValidationCode::SchemaInvalid,
                        path: i.path,
                        detail: i.code,
                    })
                    .collect(),
            );
        }
    };
    let (content, dropped) = validate_weekly(&out, input, ev);
    if dropped.is_empty() {
        Checked::Clean(content)
    } else {
        Checked::Partial(content, dropped)
    }
}

/// Writes the weekly review: attempt, validate, repair once, drop what still fails.
/// Client errors (`Unavailable`, `Budget` from the hook, which runs before each call) come back
/// as a [`SectionError`] with the records of the calls already paid for.
pub async fn generate_weekly_review(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    input: &WeeklyReviewInput,
) -> Result<WeeklyOutcome, SectionError> {
    let ev = build_weekly_evidence(input);
    let req = build_weekly_request(input);
    let Attempted {
        content,
        dropped,
        calls,
        ..
    } = attempt_and_repair(ai, before_call, &req, WEEKLY_SECTION, |r| {
        check(r, input, &ev)
    })
    .await?;
    Ok(WeeklyOutcome {
        content: content.unwrap_or_else(empty_review),
        dropped,
        calls,
    })
}
