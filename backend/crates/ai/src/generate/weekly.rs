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

use super::attempt::{Attempted, Checked, attempt_and_repair, parsed, repair_once};
use super::context::canonical_json_escaped;
use super::payload::PayloadDroppedItem;
use super::request::rules_block;
use super::section::{BeforeCall, CallRecord, SectionError};
use crate::client::model::{Effort, Models};
use crate::client::port::{
    AiClient, AiError, Message, Role, StructuredRequest, StructuredResult, TextBlock,
};
use crate::output::codes::{DroppedItem, ValidationCode, ValidationError};
use crate::output::report::{Item, ValidatedItem};
use crate::output::schema::{WEEKLY_REVIEW_SCHEMA, parse_weekly_review};
use crate::output::weekly::{
    FocusUpdate, ValidatedWeeklyOutcome, ValidatedWeeklyReview, WeeklyOutcomeItem,
    WeeklyReviewInput, WeeklyReviewOutput,
};
use crate::prompts::{WEEKLY_PROMPT_SET, load_prompt};
use crate::validator::evidence::{
    Evidence, build_evidence_from, list_date_paths, list_number_paths, path_segment,
};
use crate::validator::language::{
    banned_phrases, engine_claims, folded_words, likelihood, likelihood_phrase, no_causal_phrases,
    no_change_words, no_marketing, no_quantity_words, outcome_direction,
};
use crate::validator::text::{TextCtx, TextRule, TextRules, validate_text};
use crate::validator::tokens::TokenBody;

/// `max_tokens` of a weekly review call.
pub const WEEKLY_MAX_TOKENS: u32 = 4000;

/// `DroppedItem::section` of the weekly review.
pub const WEEKLY_SECTION: &str = "weekly";

/// The detail of a process text that quotes an outcome number (ADR 0060: a reuse of
/// `likelihood_mismatch`, since an outcome number in the process text claims an outcome). The
/// other non-outcome items use the same wording with their own name.
pub const PROCESS_OUTCOME_NUMBER: &str = "process quotes an outcome number";

/// The phrase an outcome must use when less than half the planned practice was done
/// (improvement-engine §6.6): no efficacy verdict then.
pub const NOT_ENOUGH_PRACTICE: &str = "not enough practice to judge";

/// The number paths that state an outcome: the metric deltas, the timeline changes, the
/// focus estimates, and (ADR 0094) the plan's focus progress and "Is it working?" results. Only
/// outcomes may quote them (T15 fix round 1, review I2). The plan's training counts are
/// process numbers.
const OUTCOME_PATHS: [&str; 6] = [
    "metricDeltas",
    "changes",
    "focusUpdates",
    "plan.focus",
    "plan.isItWorking",
    "plan.patterns",
];

/// Less than half the planned minutes were done (improvement-engine §6.6). `false` when nothing
/// was planned (`adherence` null or 0 minutes planned).
pub fn below_practice_gate(input: &WeeklyReviewInput) -> bool {
    input.adherence.is_some_and(|a| {
        a.minutes_planned > 0 && u64::from(a.minutes_done) * 2 < u64::from(a.minutes_planned)
    })
}

/// The likelihood phrase an outcome about `f` must use: none below the practice gate.
fn focus_phrase(input: &WeeklyReviewInput, f: &FocusUpdate) -> Option<&'static str> {
    if below_practice_gate(input) {
        None
    } else {
        likelihood_phrase(f.p_imp)
    }
}

/// The prompt `id` at its version in [`WEEKLY_PROMPT_SET`] (version 1 for any other id).
fn text(id: &str) -> &'static str {
    let v = WEEKLY_PROMPT_SET
        .iter()
        .find(|(i, _)| *i == id)
        .map_or(1, |(_, v)| *v);
    load_prompt(id, v).unwrap_or_else(|e| panic!("{e}")).text
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
    let mut ev = build_evidence_from(&input_json(input), &exemplars, &changes, &rates);
    if let Some(p) = &input.plan {
        crate::plan::add_plan_evidence(&mut ev, p);
    }
    ev
}

/// What the weekly prompts read: the input, plus
/// - `likelihoodPhrase` on each focus (null without `pImp`, and below the practice gate);
/// - `enoughPracticeToJudge` (false below the practice gate);
/// - `refs` (the `num` and `date` token paths, sorted).
pub fn build_weekly_context(input: &WeeklyReviewInput) -> Value {
    let mut ctx = input_json(input);
    if let Some(Value::Array(fs)) = ctx.get_mut("focusUpdates") {
        for (f, v) in input.focus_updates.iter().zip(fs.iter_mut()) {
            if let Value::Object(o) = v {
                o.insert("likelihoodPhrase".into(), json!(focus_phrase(input, f)));
            }
        }
    }
    ctx["enoughPracticeToJudge"] = json!(!below_practice_gate(input));
    let ev = build_weekly_evidence(input);
    ctx["refs"] = json!({
        "num": list_number_paths(&ev),
        "date": list_date_paths(&ev),
    });
    ctx
}

/// The weekly review request: `Models::WEEKLY`, low effort, [`WEEKLY_MAX_TOKENS`].
/// - `system[0]`: [`rules_block`] (cached, shared with reports and the chat);
/// - `system[1]`: `weekly-review/system` + `tone/voice` (the coach's voice) +
///   `weekly-review/output` (cached);
/// - user: `<weekly_input>` + the canonical context (cached), then `weekly-review/task`.
pub fn build_weekly_request(input: &WeeklyReviewInput) -> StructuredRequest {
    StructuredRequest {
        model: Models::WEEKLY,
        system: vec![
            rules_block(),
            TextBlock::cached(format!(
                "{}\n\n{}\n\n{}",
                text("weekly-review/system"),
                text("tone/voice"),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeeklyItemKind<'a> {
    Headline,
    Process,
    /// An outcome about this focus.
    Outcome(&'a FocusUpdate),
    Exemplar,
    NextWeek,
}

impl WeeklyItemKind<'_> {
    fn name(self) -> &'static str {
        match self {
            WeeklyItemKind::Headline => "headline",
            WeeklyItemKind::Process => "process",
            WeeklyItemKind::Outcome(_) => "outcome",
            WeeklyItemKind::Exemplar => "exemplar",
            WeeklyItemKind::NextWeek => "nextWeek",
        }
    }
}

/// A non-outcome text may not quote a number under `metricDeltas`, `changes` or `focusUpdates`
/// (`likelihood_mismatch`, "<name> quotes an outcome number").
fn no_outcome_numbers(name: &'static str) -> TextRule {
    Box::new(move |_: &str, ctx: &TextCtx| {
        let quoted = ctx.tokens.iter().any(|t| {
            matches!(&t.body, TokenBody::Num { path, .. }
                if OUTCOME_PATHS.iter().any(|r| path == r || path.starts_with(&format!("{r}."))))
        });
        if quoted {
            vec![(
                ValidationCode::LikelihoodMismatch,
                format!("{name} quotes an outcome number"),
            )]
        } else {
            Vec::new()
        }
    })
}

/// An outcome below the practice gate says so (`likelihood_mismatch`).
fn states_not_enough_practice() -> TextRule {
    Box::new(|stripped: &str, _: &TextCtx| {
        if stripped.to_lowercase().contains(NOT_ENOUGH_PRACTICE) {
            Vec::new()
        } else {
            vec![(
                ValidationCode::LikelihoodMismatch,
                format!("missing {NOT_ENOUGH_PRACTICE:?}"),
            )]
        }
    })
}

/// The rules of one weekly item (ADR 0060 "Weekly review and change notes"):
/// - every item: `banned_phrases` and `no_marketing` (TS B4), plus the report's `Item` rules
///   (`engine_claims`, `no_quantity_words`, `no_causal_phrases`);
/// - an outcome: `likelihood` and `outcome_direction` with the phrase of its own focus (named by
///   `weaknessId`): a direction word needs "very likely" or "likely" in the text, and is
///   forbidden with any other phrase; below the practice gate, no phrase at all, no direction
///   word and [`NOT_ENOUGH_PRACTICE`] instead;
/// - every other item: `likelihood(None)`, `no_change_words` and no number under
///   `metricDeltas`, `changes` or `focusUpdates`, since outcome claims belong in the outcomes;
/// - an exemplar item: a citation (tied to the week's exemplars by [`validate_weekly`]).
pub fn weekly_text_rules(input: &WeeklyReviewInput, kind: WeeklyItemKind<'_>) -> TextRules {
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
    if let WeeklyItemKind::Outcome(f) = kind {
        rules.extra.push(likelihood(focus_phrase(input, f)));
        rules.extra.push(outcome_direction(focus_phrase(input, f)));
        if below_practice_gate(input) {
            rules.extra.push(states_not_enough_practice());
        }
        return rules;
    }
    rules.require_citation = kind == WeeklyItemKind::Exemplar;
    rules.extra.push(likelihood(None));
    rules.extra.push(no_change_words());
    rules.extra.push(no_outcome_numbers(kind.name()));
    rules
}

/// Words too common in chess coaching to name a focus on their own.
const GENERIC_WORDS: [&str; 14] = [
    "piece", "game", "move", "time", "play", "your", "with", "from", "after", "before", "loss",
    "when", "over", "into",
];

/// The words of a focus label that name it: folded ([`folded_words`]), at least 4 letters, and
/// not generic. "Back-rank threats" gives `back`, `rank` and `threat`, so "back rank" or
/// "back‑rank threat" in another focus's outcome is caught (T15 fix round 2, N2).
fn distinctive_words(label: &str) -> Vec<String> {
    folded_words(label)
        .into_iter()
        .filter(|w| w.chars().count() >= 4 && !GENERIC_WORDS.contains(&w.as_str()))
        .collect()
}

fn verr(code: ValidationCode, path: String, detail: String) -> ValidationError {
    ValidationError { code, path, detail }
}

struct V<'a> {
    input: &'a WeeklyReviewInput,
    ev: &'a Evidence,
    exemplars: HashSet<String>,
    dropped: Vec<DroppedItem>,
}

impl V<'_> {
    fn drop_item(&mut self, path: &str, errors: Vec<ValidationError>) {
        self.dropped.push(DroppedItem {
            section: WEEKLY_SECTION.to_owned(),
            path: path.to_owned(),
            errors,
        });
    }

    /// The text's segments, or every error of the paragraph (plus `unrelated_citation` for an
    /// exemplar item citing a position outside the week's exemplars).
    fn checked(
        &self,
        text: &str,
        citations: &[crate::output::codes::Citation],
        kind: WeeklyItemKind<'_>,
        path: &str,
    ) -> Result<Vec<crate::segments::Segment>, Vec<ValidationError>> {
        let rules = weekly_text_rules(self.input, kind);
        let result = validate_text(text, citations, self.ev, &rules, &format!("{path}.text"));
        let mut unrelated = Vec::new();
        if kind == WeeklyItemKind::Exemplar {
            for (i, c) in citations.iter().enumerate() {
                let k = format!("{}#{}", c.game_id, c.ply);
                if self.ev.positions.contains_key(&k) && !self.exemplars.contains(&k) {
                    unrelated.push(verr(
                        ValidationCode::UnrelatedCitation,
                        format!("{path}.citations[{i}]"),
                        format!("{k} is not one of the week's exemplars"),
                    ));
                }
            }
        }
        match result {
            Ok(segments) if unrelated.is_empty() => Ok(segments),
            Ok(_) => Err(unrelated),
            Err(mut e) => {
                e.extend(unrelated);
                Err(e)
            }
        }
    }

    fn item(&mut self, it: &Item, kind: WeeklyItemKind<'_>, path: &str) -> Option<ValidatedItem> {
        match self.checked(&it.text, &it.citations, kind, path) {
            Ok(segments) => Some(ValidatedItem {
                text: it.text.clone(),
                citations: it.citations.clone(),
                segments,
            }),
            Err(e) => {
                self.drop_item(path, e);
                None
            }
        }
    }

    /// An outcome: its `weaknessId` must name a focus of the week not already covered by an
    /// accepted outcome (`unknown_weakness`), its text may not use a distinctive word of another
    /// focus's label (`unknown_weakness`), and it is checked with that focus's likelihood phrase.
    fn outcome(
        &mut self,
        it: &WeeklyOutcomeItem,
        seen: &mut HashSet<String>,
        path: &str,
    ) -> Option<ValidatedWeeklyOutcome> {
        let id_path = format!("{path}.weaknessId");
        let input = self.input;
        let Some(focus) = input
            .focus_updates
            .iter()
            .find(|f| f.weakness_id == it.weakness_id)
        else {
            let e = verr(
                ValidationCode::UnknownWeakness,
                id_path,
                format!("{}: not a focus of this week", it.weakness_id),
            );
            self.drop_item(path, vec![e]);
            return None;
        };
        if seen.contains(&focus.weakness_id) {
            let e = verr(
                ValidationCode::UnknownWeakness,
                id_path,
                format!("{}: a duplicate of an accepted outcome", it.weakness_id),
            );
            self.drop_item(path, vec![e]);
            return None;
        }
        let words: HashSet<String> = folded_words(&it.text).into_iter().collect();
        let own: HashSet<String> = folded_words(&focus.label).into_iter().collect();
        let mut errors: Vec<ValidationError> = input
            .focus_updates
            .iter()
            .filter(|f| f.weakness_id != focus.weakness_id)
            .filter(|f| {
                distinctive_words(&f.label)
                    .iter()
                    .any(|w| !own.contains(w) && words.contains(w))
            })
            .map(|f| {
                verr(
                    ValidationCode::UnknownWeakness,
                    format!("{path}.text"),
                    format!("names another focus ({})", f.weakness_id),
                )
            })
            .collect();
        match self.checked(
            &it.text,
            &it.citations,
            WeeklyItemKind::Outcome(focus),
            path,
        ) {
            Ok(segments) if errors.is_empty() => {
                seen.insert(focus.weakness_id.clone());
                Some(ValidatedWeeklyOutcome {
                    text: it.text.clone(),
                    citations: it.citations.clone(),
                    segments,
                    weakness_id: focus.weakness_id.clone(),
                    label: focus.label.clone(),
                })
            }
            result => {
                if let Err(e) = result {
                    errors.splice(0..0, e);
                }
                self.drop_item(path, errors);
                None
            }
        }
    }
}

/// Validates a weekly review item by item. A failing singleton becomes `None` and a failing
/// entry is removed; each is listed with its path (`headline`, `outcomes[0]`, `nextWeek`), its
/// errors at `<path>.text` (or `<path>.weaknessId`). A validated outcome carries its focus's
/// computed label.
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
    let mut seen = HashSet::new();
    let outcomes = out
        .outcomes
        .iter()
        .enumerate()
        .filter_map(|(i, it)| v.outcome(it, &mut seen, &format!("outcomes[{i}]")))
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
    /// The audit copy, for storage only: its `detail` strings quote rejected model text. Never
    /// send it to the web; send [`WeeklyOutcome::web_dropped`] (T14 I2 ruling).
    pub dropped: Vec<DroppedItem>,
    /// Every model call made: 1 or 2. All of them are metered.
    pub calls: Vec<CallRecord>,
}

impl WeeklyOutcome {
    /// `dropped` as the web may see it: codes and paths, no `detail`.
    pub fn web_dropped(&self) -> Vec<PayloadDroppedItem> {
        self.dropped.iter().map(PayloadDroppedItem::from).collect()
    }
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

pub(crate) fn check(
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

/// The outcome of an attempted weekly review (an empty review when nothing was usable).
pub(crate) fn weekly_outcome(a: Attempted<ValidatedWeeklyReview>) -> WeeklyOutcome {
    WeeklyOutcome {
        content: a.content.unwrap_or_else(empty_review),
        dropped: a.dropped,
        calls: a.calls,
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
    let a = attempt_and_repair(ai, before_call, &req, WEEKLY_SECTION, |r| {
        check(r, input, &ev)
    })
    .await?;
    Ok(weekly_outcome(a))
}

/// The one repair of a weekly review whose first result (`first`, synchronous or from a batch)
/// failed with `errors`: the standalone entry of the batch path (Task 16), with the same rules
/// as [`generate_weekly_review`]'s repair. `calls` holds only the repair call: the caller
/// records `first`. A client error is returned as is; the caller still meters `first`.
pub async fn repair_weekly_review(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    input: &WeeklyReviewInput,
    first: &StructuredResult,
    errors: Vec<ValidationError>,
) -> Result<WeeklyOutcome, AiError> {
    let ev = build_weekly_evidence(input);
    let req = build_weekly_request(input);
    let a = repair_once(ai, before_call, &req, WEEKLY_SECTION, first, errors, |r| {
        check(r, input, &ev)
    })
    .await?;
    Ok(weekly_outcome(a))
}
