//! Coaching language rules (TS plan A5, improvement-engine §6.7, spec §8 cont., §12b), in their
//! token forms (ADR-R3-4 "Language rules"), and the rule set each report paragraph gets.
//!
//! Every rule reads the stripped text, where each token is `§`. The patterns are the TS plan's
//! regexes plus the fix round 1 additions, written JS-style and translated by `js`: JS `\b` is
//! ASCII, `\s` is JS's whitespace set, and `/i` matches what JS matches on the NFKC text.
//!
//! Union rule (ADR-R3-2). A rule that *forbids* a phrase runs on the stripped text and on its
//! hardened normalisation (homoglyphs, marks and every invisible folded away; computed once per
//! paragraph, [`TextCtx::hardened`]), and fires if either matches, so `guаranteed` with a
//! Cyrillic `а` is still banned. A rule that *requires* a phrase (a hedge, the time-class word,
//! the likelihood phrase before a direction word) reads only the stripped text, the form TS
//! reads: folding could only make a missing phrase appear.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::bounded::{JS_SPACE, count_scanned};
use super::format::format_number;
use super::text::{TextCtx, TextRule, TextRules};
use super::tokens::{NumFormat, PvSide, TokenBody};
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

/// Builds a rule pattern from JS-style source: `(?i:…)` around it, `\b` as the ASCII
/// `(?-u:\b)`, `\s` as JS's whitespace set and `\w` as `[A-Za-z0-9_]`. Unicode case folding adds
/// only U+212A and U+017F to ASCII letters, and NFKC (in the token canonical form) has removed
/// both, so `(?i:…)` matches what JS `/i` matches.
fn js(src: &str) -> Regex {
    let body = src
        .replace(r"\b", r"(?-u:\b)")
        .replace(r"\s", &format!("[{JS_SPACE}]"))
        .replace(r"[[", "[")
        .replace(r"]-]", "-]")
        .replace(r"\w", "[A-Za-z0-9_]");
    re(&format!("(?i:{body})"))
}

macro_rules! lazy_re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| js($pat));
    };
}

// TS plan A5, with fix round 1 (C5): `\s+` between words, `'ll` and `is going to`,
// `guarantee\w*`, `certain(ty)?` and the near-certainty words.
lazy_re!(GUARANTEE, r"\bguarantee\w*");
lazy_re!(DEFINITELY, r"\bdefinitely\b");
lazy_re!(CERTAIN, r"\bcertain(ty|ly)?\b");
lazy_re!(PROVEN, r"\bproven?\b");
lazy_re!(
    NEAR_CERTAIN,
    r"\b(almost\s+(surely|certain(ly)?)|virtually\s+(certain(ly)?|guaranteed)|surely|no\s+doubt|without\s+(a\s+)?doubt|bound\s+to|sure\s+to)\b"
);
lazy_re!(
    WILL_GAIN,
    r"(\bwill|['’]ll|\b(is|are)\s+going\s+to)\s+(gain|reach|get\s+you\s+to|raise\s+your\s+rating|improve\s+your\s+rating)\b"
);
// Fix round 1 (I9) adds `pro (tier|version|account)`, `buy`, `unlock`, `pricing`, `paywall`.
lazy_re!(
    MARKETING,
    r"\b(upgrade|subscribe|subscription|premium|pro\s+(plan|tier|version|account)|discount|free\s+trial|refer\s+a\s+friend|share\s+your\s+(report|card)|buy|unlock|pricing|paywall)\b"
);
lazy_re!(
    CAUSAL,
    r"\b(caused|because(\s+of)?|due\s+to|the\s+reason\s+(is|was)|led\s+to|proves?)\b"
);
lazy_re!(
    HEDGE,
    r"\b(may|might|possibly|could|coincid\w*|around\s+the\s+time|at\s+about\s+the\s+same\s+time)\b"
);
// Longest first, so "very likely" is one phrase and "unlikely" none (`\b`). Fix round 1 (C5):
// "highly/most/extremely/very likely", also hyphenated, are phrases of their own.
lazy_re!(
    LIKELIHOOD,
    r"\b(no\s+clear\s+change\s+yet|(highly|most|extremely|very)[\s-]+likely|unclear\s+yet|likely)\b"
);
// Fix round 1 (I7): a direction of change needs the likelihood phrase. Fix round 2 adds the
// nouns and "stopped" ("a clear improvement", "this has stopped"): both assert a change.
lazy_re!(
    DIRECTION,
    r"\b(improving|improved|improvements?|getting\s+(better|worse)|fixed|worse|worsening|declining|declined|stopped)\b"
);
// Fix round 1 (C4): engine and evaluation words.
lazy_re!(
    ENGINE,
    r"\b(engine|stockfish|winning|losing|(check)?mate[sd]?|advantage|equal|top\s+choice|best\s+move|threw\s+away)\b"
);
// R3 Task 15: the direction words of a change in an event rate, for change notes and the
// non-outcome weekly items. Fix round 1 (review I1) adds the bare comparatives and the
// everyday verbs ("more", "spiked", "again"; "less", "rarely", "eased"): a bare `more` or
// `less` fails some honest texts, the accepted side to fail on.
lazy_re!(
    RATE_UP,
    r"\b(more|increas\w*|ris(e|es|en|ing)|rose|(went|gone|go|goes|going)\s+up|higher|worse\w*|worsen\w*|regress\w*|(came|come|comes|coming)\s+back|return\w*|spik\w*|climb\w*|grow\w*|grew|jump\w*|bigger|larger|again|doubl\w*|tripl\w*|surg\w*|soar\w*|(crept|creep\w*)\s+up|pick(ed|ing)?\s+up)\b"
);
lazy_re!(
    RATE_DOWN,
    r"\b(less|fewer|decreas\w*|drop\w*|fell|fall\w*|(went|gone|go|goes|going)\s+down|lower\w*|reduc\w*|improv\w*|better|resolved|fixed|stopped|declin\w*|rare\w*|no\s+longer|gone\s+away|(went|go|goes|going)\s+away|eas(e|ed|es|ing)|dip\w*|shr[iu]nk\w*|shrank|halv\w*|smaller|fad(e|ed|es|ing)|vanish\w*|disappear\w*|cut\s+down|settl\w*\s+down)\b"
);
lazy_re!(BLUNDER, r"\b(blunder|mistake)\w*");
// Fix round 1 (C6): quantities and absolutes in words.
lazy_re!(
    QUANTITY,
    r"\b(most\s+(of\s+your\s+)?games|the\s+majority|nearly\s+every|every\s+(single\s+)?game|each\s+game|a\s+handful|a\s+score\s+of|\w*fold|nil|always|never)\b"
);

/// Lowercased, with each run of spaces and hyphens one space.
fn canon(m: &str) -> String {
    let mut out = String::with_capacity(m.len());
    let mut gap = false;
    for c in m.chars() {
        if c == '-' || super::bounded::is_js_space(c) {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Every match of `re` in the stripped text and in its hardened form, canonicalised, each once,
/// in that order (the union rule).
fn find_all(re: &Regex, stripped: &str, ctx: &TextCtx) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for f in [stripped, ctx.hardened] {
        count_scanned(f.len());
        for m in re.find_iter(f) {
            let s = canon(m.as_str());
            if seen.insert(s.clone()) {
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
    Box::new(move |stripped: &str, ctx: &TextCtx| {
        patterns
            .iter()
            .flat_map(|p| find_all(p, stripped, ctx))
            .map(|m| (code, m))
            .collect()
    })
}

/// No guarantees, no certainty, no promised rating gains (`banned_phrase`).
pub fn banned_phrases() -> TextRule {
    forbid(
        vec![
            &GUARANTEE,
            &DEFINITELY,
            &CERTAIN,
            &PROVEN,
            &NEAR_CERTAIN,
            &WILL_GAIN,
        ],
        ValidationCode::BannedPhrase,
    )
}

/// No marketing or upsell copy, for anyone (minors, spec §14) (`marketing`).
pub fn no_marketing() -> TextRule {
    forbid(vec![&MARKETING], ValidationCode::Marketing)
}

/// Any likelihood phrase other than `required` (`likelihood_mismatch`). With `None`, every
/// likelihood phrase is a mismatch. A direction of change (improving, getting better, fixed,
/// worse, declining) needs the required phrase in the same text; with `None` it is always a
/// mismatch, since nothing supports a direction (fix round 1, I7).
pub fn likelihood(required: Option<&'static str>) -> TextRule {
    Box::new(move |stripped: &str, ctx: &TextCtx| {
        let found = find_all(&LIKELIHOOD, stripped, ctx);
        let mut out: Vec<(ValidationCode, String)> = found
            .iter()
            .filter(|m| Some(m.as_str()) != required)
            .map(|m| (ValidationCode::LikelihoodMismatch, m.clone()))
            .collect();
        let stated = required.is_some_and(|r| {
            count_scanned(stripped.len());
            LIKELIHOOD
                .find_iter(stripped)
                .any(|m| canon(m.as_str()) == r)
        });
        if !stated {
            for d in find_all(&DIRECTION, stripped, ctx) {
                out.push((
                    ValidationCode::LikelihoodMismatch,
                    format!("{d} without {:?}", required.unwrap_or("a likelihood")),
                ));
            }
        }
        out
    })
}

fn has_num_token(ctx: &TextCtx, path: &str) -> bool {
    ctx.tokens
        .iter()
        .any(|t| matches!(&t.body, TokenBody::Num { path: p, .. } if p == path))
}

/// The text names the evidence window: a `{{num:basis.analysedGames}}` or
/// `{{num:basis.analysedGames|int}}` token and the time-class word (`missing_evidence_window`).
pub fn evidence_window(tc: TimeClass) -> TextRule {
    let word = js(&format!(r"\b{}\b", tc.as_str()));
    Box::new(move |stripped: &str, ctx: &TextCtx| {
        let mut missing = Vec::new();
        let count = ctx.tokens.iter().any(|t| {
            matches!(&t.body, TokenBody::Num { path, fmt: None | Some(NumFormat::Int) }
                if path == "basis.analysedGames")
        });
        if !count {
            missing.push("{{num:basis.analysedGames|int}}");
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
/// with no hedge gives `missing_hedge` (change notes).
pub fn no_causal_claims() -> TextRule {
    Box::new(|stripped: &str, ctx: &TextCtx| {
        let mut out: Vec<(ValidationCode, String)> = find_all(&CAUSAL, stripped, ctx)
            .into_iter()
            .map(|m| (ValidationCode::CausalClaim, m))
            .collect();
        if !has(&HEDGE, stripped) {
            out.push((ValidationCode::MissingHedge, "no hedge".to_owned()));
        }
        out
    })
}

/// The causal phrases alone, with no hedge requirement (`causal_claim`), for report texts
/// (fix round 1, I8).
pub fn no_causal_phrases() -> TextRule {
    forbid(vec![&CAUSAL], ValidationCode::CausalClaim)
}

/// Quantities and absolutes in words ("most games", "a handful", "twofold", "always")
/// (`free_text_fact`, fix round 1, C6).
pub fn no_quantity_words() -> TextRule {
    Box::new(|stripped: &str, ctx: &TextCtx| {
        find_all(&QUANTITY, stripped, ctx)
            .into_iter()
            .map(|m| (ValidationCode::FreeTextFact, format!("quantity word {m:?}")))
            .collect()
    })
}

/// The last `n` words of `s` and the first `n` words of `t`, bounded to 64 bytes each.
fn words_around(before: &str, after: &str, n: usize) -> String {
    let mut b = before.len().saturating_sub(64);
    while !before.is_char_boundary(b) {
        b += 1;
    }
    let mut a = after.len().min(64);
    while !after.is_char_boundary(a) {
        a -= 1;
    }
    count_scanned(before.len() - b + a);
    let pre: Vec<&str> = before[b..].split_whitespace().collect();
    let post: Vec<&str> = after[..a].split_whitespace().take(n).collect();
    let pre = &pre[pre.len().saturating_sub(n)..];
    format!("{} {}", pre.join(" "), post.join(" "))
}

/// Engine and evaluation claims need engine evidence (`uncited_reference`, fix round 1, C4):
/// - a word such as engine, Stockfish, winning, losing, mate, advantage, equal, top choice, best
///   move or threw away needs, in the same paragraph, a `best`, `pv` or `eng` token, or a `num`
///   token on an evaluation path of a cited position;
/// - "blunder" or "mistake" may not sit within four words of a `best` token (or a `pv` token of
///   the best line), which would call the engine's move the player's error.
pub fn engine_claims() -> TextRule {
    Box::new(|stripped: &str, ctx: &TextCtx| {
        let mut out = Vec::new();
        let words = find_all(&ENGINE, stripped, ctx);
        if !words.is_empty() {
            let cited: HashSet<String> = ctx
                .citations
                .iter()
                .map(|c| format!("{}#{}", c.game_id, c.ply))
                .chain(ctx.tokens.iter().filter_map(|t| match &t.body {
                    TokenBody::Move { game_id, ply } => Some(format!("{game_id}#{ply}")),
                    _ => None,
                }))
                .collect();
            let supported = ctx.tokens.iter().any(|t| match &t.body {
                TokenBody::Best { .. } | TokenBody::Pv { .. } | TokenBody::Eng { .. } => true,
                TokenBody::Num { path, .. } => {
                    path.split('.')
                        .any(|s| s.to_ascii_lowercase().starts_with("eval"))
                        && ctx
                            .evidence
                            .position_of_path(path)
                            .is_some_and(|k| cited.contains(k))
                }
                _ => false,
            });
            if !supported {
                out.extend(words.into_iter().map(|w| {
                    (
                        ValidationCode::UncitedReference,
                        format!("engine claim {w:?} without engine evidence"),
                    )
                }));
            }
        }
        for t in ctx.tokens {
            let best = matches!(
                &t.body,
                TokenBody::Best { .. }
                    | TokenBody::Pv {
                        side: PvSide::Best,
                        ..
                    }
            );
            if !best {
                continue;
            }
            let Some(before) = ctx.canonical.get(..t.index) else {
                continue;
            };
            let Some(after) = ctx.canonical.get(t.index + t.raw.len()..) else {
                continue;
            };
            let near = words_around(before, after, 4);
            if let Some(m) = BLUNDER.find(&near) {
                out.push((
                    ValidationCode::UncitedReference,
                    format!(
                        "{:?} next to the engine's move {}",
                        canon(m.as_str()),
                        t.raw
                    ),
                ));
            }
        }
        out
    })
}

/// A change note describes the change R2 detected, in its own direction (R3 Task 15): when the
/// event rate went down, a word of increase or worsening ("more often", "worse", "came back") is
/// a claim the timeline does not support; when it went up, a word of decrease or improvement
/// ("fewer", "improved", "resolved"); with equal rates, both (`free_text_fact`). A forbidding
/// rule, so it runs on the stripped and the hardened text.
pub fn change_direction(rate_before: f64, rate_after: f64) -> TextRule {
    let mut forbidden: Vec<&'static LazyLock<Regex>> = Vec::new();
    if rate_after <= rate_before {
        forbidden.push(&RATE_UP);
    }
    if rate_after >= rate_before {
        forbidden.push(&RATE_DOWN);
    }
    Box::new(move |stripped: &str, ctx: &TextCtx| {
        forbidden
            .iter()
            .flat_map(|p| find_all(p, stripped, ctx))
            .map(|m| {
                (
                    ValidationCode::FreeTextFact,
                    format!("{m:?} is not the direction of the detected change"),
                )
            })
            .collect()
    })
}

/// No word of change in either direction (T15 fix round 1, review I2): for weekly texts that
/// are not outcomes, where a change is an outcome claim without its likelihood phrase
/// (`likelihood_mismatch`). A forbidding rule.
pub fn no_change_words() -> TextRule {
    Box::new(|stripped: &str, ctx: &TextCtx| {
        [&RATE_UP, &RATE_DOWN]
            .into_iter()
            .flat_map(|p| find_all(p, stripped, ctx))
            .map(|m| {
                (
                    ValidationCode::LikelihoodMismatch,
                    format!("{m:?} claims a change outside an outcome"),
                )
            })
            .collect()
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
    /// rhythm or a drill rationale.
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

/// TS `reportTextRules`, in token form, with fix rounds 1 and 2.
/// - Everywhere: `banned_phrases`, `no_marketing`, `engine_claims` and `no_quantity_words`.
/// - Weakness and Focus: a citation, `likelihood(likelihood_phrase(w.p_imp))` (an unknown or
///   general weakness requires no phrase, so every phrase and direction is a mismatch),
///   `evidence_window` and `no_causal_phrases`.
/// - Summary: `evidence_window` and `no_causal_phrases`.
/// - Area: below its minimum sample, `states_insufficient_data`.
/// - Item: `no_causal_phrases`.
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
            extra: vec![
                banned_phrases(),
                no_marketing(),
                engine_claims(),
                no_quantity_words(),
            ],
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
                rules.extra.push(no_causal_phrases());
            }
            RuleKind::Summary => {
                rules.extra.push(evidence_window(tc));
                rules.extra.push(no_causal_phrases());
            }
            RuleKind::Area => {
                if let Some(area) = meta.area.filter(|a| insufficient.contains(a)) {
                    rules.extra.push(states_insufficient_data(area));
                }
            }
            RuleKind::Opening => {
                rules.require_citation = true;
                rules.line_keys = Some(meta.line_key.into_iter().collect());
            }
            RuleKind::Item => rules.extra.push(no_causal_phrases()),
        }
        rules
    }
}
