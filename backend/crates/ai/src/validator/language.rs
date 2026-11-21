//! Coaching language rules (TS plan A5, improvement-engine §6.7, spec §8 cont., §12b), in their
//! token forms (ADR-R3-4 "Language rules"), and the rule set each report paragraph gets.
//!
//! Every rule reads the stripped text, where each token is `§`. The patterns are the TS plan's
//! regexes. JS `/i` without the `u` flag folds ASCII letters only, and JS `\b` is ASCII, so each
//! pattern is `(?i-u:…)`: the same matches as JS.
//!
//! Union rule (ADR-R3-2). A rule that *forbids* a phrase runs on the stripped text and on its
//! hardened normalisation (homoglyphs, marks and every invisible folded away), and fires if either
//! matches, so `guаranteed` with a Cyrillic `а` is still banned. A rule that *requires* a phrase
//! (a hedge, the time-class word) reads only the stripped text, the form TS reads: folding could
//! only make a missing phrase appear.

use std::sync::LazyLock;

use regex::Regex;

use super::bounded::count_scanned;
use super::format::format_number;
use super::normalize::normalize_text;
use super::text::{TextCtx, TextRule, TextRules};
use super::tokens::{NumFormat, TokenBody};
use crate::input::types::{AreaId, Basis, CoachingInput, TimeClass};
use crate::output::codes::ValidationCode;

/// TS `likelihoodPhrase`: the only likelihood words a weakness or focus text may use.
pub fn likelihood_phrase(p_imp: Option<f64>) -> Option<&'static str> {
    let p = p_imp?;
    Some(if p >= 0.95 {
        "very likely"
    } else if p >= 0.85 {
        "likely"
    } else if p >= 0.5 {
        "unclear yet"
    } else {
        "no clear change yet"
    })
}

/// TS `confidencePhrase`.
pub fn confidence_phrase(c: f64) -> &'static str {
    if c >= 0.9 {
        "clearly"
    } else if c >= 0.7 {
        "probably"
    } else {
        "possibly"
    }
}

/// The evidence window in token form, for prompts and model text.
pub fn evidence_window_token(time_class: TimeClass) -> String {
    format!(
        "over your last {{{{num:basis.analysedGames|int}}}} analysed {} games ({{{{num:basis.importedGames|int}}}} imported)",
        time_class.as_str()
    )
}

/// TS `formatEvidenceWindow`, for computed (non-model) text only.
pub fn format_evidence_window(basis: &Basis, time_class: TimeClass) -> String {
    let int = |n: u32| format_number(f64::from(n), Some(NumFormat::Int)).unwrap_or_default();
    format!(
        "over your last {} analysed {} games ({} imported)",
        int(basis.analysed_games),
        time_class.as_str(),
        int(basis.imported_games)
    )
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("bad pattern {p}: {e}"))
}

macro_rules! lazy_re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| re($pat));
    };
}

lazy_re!(GUARANTEE, r"(?i-u:\bguarantee[ds]?\b)");
lazy_re!(DEFINITELY, r"(?i-u:\bdefinitely\b)");
lazy_re!(CERTAINLY, r"(?i-u:\bcertainly\b)");
lazy_re!(PROVEN, r"(?i-u:\bproven?\b)");
lazy_re!(
    WILL_GAIN,
    r"(?i-u:\bwill (gain|reach|get you to|raise your rating|improve your rating)\b)"
);
lazy_re!(
    MARKETING,
    r"(?i-u:\b(upgrade|subscribe|subscription|premium|pro plan|discount|free trial|refer a friend|share your (report|card))\b)"
);
lazy_re!(
    CAUSAL,
    r"(?i-u:\b(caused|because of|due to|the reason (is|was)|led to|proves?)\b)"
);
lazy_re!(
    HEDGE,
    r"(?i-u:\b(may|might|possibly|could|coincid[A-Za-z0-9_]*|around the time|at about the same time)\b)"
);
// Longest first, so "very likely" is one phrase and "unlikely" none (`\b`).
lazy_re!(
    LIKELIHOOD,
    r"(?i-u:\b(no clear change yet|very likely|unclear yet|likely)\b)"
);

/// The forbidden-phrase forms of a stripped text: itself, and its hardened normalisation when
/// that differs.
fn forms(stripped: &str) -> Vec<String> {
    count_scanned(stripped.len());
    let hard = normalize_text(stripped);
    if hard == stripped {
        vec![stripped.to_owned()]
    } else {
        vec![stripped.to_owned(), hard]
    }
}

/// Every match of `re` in every form, as lowercased fragments, each once, in form order.
fn find_all(re: &Regex, forms: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in forms {
        count_scanned(f.len());
        for m in re.find_iter(f) {
            let s = m.as_str().to_ascii_lowercase();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

fn has(re: &Regex, stripped: &str) -> bool {
    count_scanned(stripped.len());
    re.is_match(stripped)
}

fn forbid(patterns: Vec<&'static LazyLock<Regex>>, code: ValidationCode) -> TextRule {
    Box::new(move |stripped: &str, _: &TextCtx| {
        let forms = forms(stripped);
        patterns
            .iter()
            .flat_map(|p| find_all(p, &forms))
            .map(|m| (code, m))
            .collect()
    })
}

/// No guarantees, no certainty, no promised rating gains (`banned_phrase`).
pub fn banned_phrases() -> TextRule {
    forbid(
        vec![&GUARANTEE, &DEFINITELY, &CERTAINLY, &PROVEN, &WILL_GAIN],
        ValidationCode::BannedPhrase,
    )
}

/// No marketing or upsell copy, for anyone (minors, spec §14) (`marketing`).
pub fn no_marketing() -> TextRule {
    forbid(vec![&MARKETING], ValidationCode::Marketing)
}

/// Any likelihood phrase other than `required` (`likelihood_mismatch`). With `None`, every
/// likelihood phrase is a mismatch. The text need not use the phrase.
pub fn likelihood(required: Option<&'static str>) -> TextRule {
    Box::new(move |stripped: &str, _: &TextCtx| {
        find_all(&LIKELIHOOD, &forms(stripped))
            .into_iter()
            .filter(|m| Some(m.as_str()) != required)
            .map(|m| (ValidationCode::LikelihoodMismatch, m))
            .collect()
    })
}

fn has_num_token(ctx: &TextCtx, path: &str) -> bool {
    ctx.tokens
        .iter()
        .any(|t| matches!(&t.body, TokenBody::Num { path: p, .. } if p == path))
}

/// The text names the evidence window: a `{{num:basis.analysedGames…}}` token and the time-class
/// word (`missing_evidence_window`).
pub fn evidence_window(tc: TimeClass) -> TextRule {
    let word = re(&format!(r"(?i-u:\b{}\b)", tc.as_str()));
    Box::new(move |stripped: &str, ctx: &TextCtx| {
        let mut missing = Vec::new();
        if !has_num_token(ctx, "basis.analysedGames") {
            missing.push("{{num:basis.analysedGames}}");
        }
        if !has(&word, stripped) {
            missing.push(tc.as_str());
        }
        if missing.is_empty() {
            Vec::new()
        } else {
            vec![(
                ValidationCode::MissingEvidenceWindow,
                format!("missing {}", missing.join(" and ")),
            )]
        }
    })
}

/// Correlated causes are possible, never proven: a causal phrase gives `causal_claim`, and a text
/// with no hedge gives `missing_hedge`.
pub fn no_causal_claims() -> TextRule {
    Box::new(|stripped: &str, _: &TextCtx| {
        let mut out: Vec<(ValidationCode, String)> = find_all(&CAUSAL, &forms(stripped))
            .into_iter()
            .map(|m| (ValidationCode::CausalClaim, m))
            .collect();
        if !has(&HEDGE, stripped) {
            out.push((ValidationCode::MissingHedge, "no hedge".to_owned()));
        }
        out
    })
}

/// An area below its minimum sample says how many more games it needs: a
/// `{{num:areaScores.<area>.needMore…}}` token (`insufficient_data_not_stated`).
pub fn states_insufficient_data(area: AreaId) -> TextRule {
    let path = format!("areaScores.{}.needMore", area.as_str());
    Box::new(move |_: &str, ctx: &TextCtx| {
        if has_num_token(ctx, &path) {
            Vec::new()
        } else {
            vec![(
                ValidationCode::InsufficientDataNotStated,
                format!("missing {{{{num:{path}}}}}"),
            )]
        }
    })
}

/// Which paragraph of a report a rule set is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleKind {
    /// Any other paragraph: the headline, a strength, the plan intro, a habit, the weekly
    /// rhythm, a weakness title or a drill rationale.
    Item,
    Weakness,
    /// A focus's `why`.
    Focus,
    Area,
    /// The overview summary.
    Summary,
    Opening,
}

/// What the paragraph is about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleMeta {
    pub weakness_id: Option<String>,
    pub area: Option<AreaId>,
    pub line_key: Option<String>,
}

/// TS `reportTextRules`, in token form. Everywhere: `banned_phrases` and `no_marketing`.
/// - Weakness and Focus: a citation, `likelihood(likelihood_phrase(w.p_imp))` (an unknown or
///   general weakness requires no phrase, so every phrase is a mismatch) and `evidence_window`.
/// - Summary: `evidence_window`.
/// - Area below its minimum sample: `states_insufficient_data`.
/// - Opening: a citation, and `line` tokens only for its own line.
pub fn report_text_rules(
    input: &CoachingInput,
) -> impl Fn(RuleKind, RuleMeta) -> TextRules + Send + Sync + use<> {
    let tc = input.time_class;
    let p_imp: Vec<(String, Option<f64>)> = input
        .weaknesses
        .iter()
        .map(|w| (w.id.clone(), w.p_imp))
        .collect();
    let insufficient: Vec<AreaId> = input
        .area_scores
        .iter()
        .filter(|a| !a.sufficient)
        .map(|a| a.area)
        .collect();
    move |kind: RuleKind, meta: RuleMeta| {
        let mut rules = TextRules {
            extra: vec![banned_phrases(), no_marketing()],
            ..TextRules::default()
        };
        match kind {
            RuleKind::Weakness | RuleKind::Focus => {
                rules.require_citation = true;
                let p = meta
                    .weakness_id
                    .as_deref()
                    .and_then(|id| p_imp.iter().find(|(w, _)| w == id).and_then(|(_, p)| *p));
                rules.extra.push(likelihood(likelihood_phrase(p)));
                rules.extra.push(evidence_window(tc));
            }
            RuleKind::Summary => rules.extra.push(evidence_window(tc)),
            RuleKind::Area => {
                if let Some(area) = meta.area.filter(|a| insufficient.contains(a)) {
                    rules.extra.push(states_insufficient_data(area));
                }
            }
            RuleKind::Opening => {
                rules.require_citation = true;
                rules.line_keys = Some(meta.line_key.into_iter().collect());
            }
            RuleKind::Item => {}
        }
        rules
    }
}
