//! The claim validator for one paragraph of token text (ADR 0060 "Claim validator"). It decides
//! whether a paragraph Claude wrote is safe to show and, when it is, renders its segments.
//!
//! The checks, in order (fix round 1 collection rules):
//! 1. Citations: each is a key of `positions` (`unknown_citation`); `require_citation` with none
//!    gives `missing_citation`.
//! 2. Token syntax: any [`ParsedTokens::errors`] entry, and `{{offtopic}}` anywhere but as the
//!    whole paragraph (`token_syntax`). Prose inside a malformed `{{…}}` is never leak-scanned.
//!    **This is the only hard barrier**: with a syntax error the result is stages 1 and 2.
//! 3. Leaks: [`find_leaks`] on the stripped text (`free_text_fact`), the TS/hardened union,
//!    without ordinals used as discourse ([`is_discourse_ordinal`], R2-1 (b)).
//! 4. Defence in depth, only when stage 3 found nothing: [`extract_moves`], [`extract_numbers`]
//!    and [`extract_dates`] on the stripped text (`unknown_move`, `unknown_number`,
//!    `unknown_date`). Any result is a fact outside a token. `normalize_text` alone (hardened
//!    only) is never used for a fact check.
//! 5. Tokens: every token resolves, and a `num` token's format suits its value and path
//!    ([`num_format_allowed`]) (`unresolved_token`); no token is glued to a letter, a digit, a
//!    notation character or another token ([`glued`]) (`glued_token`).
//! 6. Scoping: unless `implicit_citations`, every `move`/`best`/`pv` token's `gameId#ply` is one
//!    of the citations; every `sq` token names a square of a recorded move or engine line of a
//!    cited position (`uncited_reference`).
//! 7. `line` tokens only for keys in `line_keys`, when it is `Some` (`unknown_line`).
//! 8. The `extra` rules (language rules).
//!
//! Stages 3 and 5–8 all run, and every error is reported, sorted by a stable key (code order,
//! then path, then detail). Any error rejects the paragraph.
//!
//! Callers pass schema-checked text of at most `ITEM_TEXT_MAX_CHARS` code points (checked by a
//! debug assertion). The cost is linear in the text either way (`count_scanned`).

use std::collections::HashSet;

use unicode_normalization::char::is_combining_mark;

use super::bounded::count_scanned;
use super::evidence::Evidence;
use super::extract::{extract_dates, extract_moves, extract_numbers};
use super::format::resolve_token;
use super::leaks::{find_leaks, is_discourse_ordinal};
use super::normalize::{is_dash, is_invisible, normalize_text};
use super::segments_render::render_segments;
use super::tokens::{NumFormat, Token, TokenBody, canonical_for_tokens, parse_tokens};
use crate::output::codes::{Citation, ValidationCode, ValidationError};
use crate::output::schema::ITEM_TEXT_MAX_CHARS;
use crate::segments::Segment;

/// What a rule sees besides the stripped text. The paragraph is normalised once.
pub struct TextCtx<'a> {
    pub tokens: &'a [Token],
    pub citations: &'a [Citation],
    pub evidence: &'a Evidence,
    /// The canonical text the token indexes point into.
    pub canonical: &'a str,
    /// `normalize_text(stripped)`: the hardened form, for the union rule of forbidding rules.
    pub hardened: &'a str,
}

/// A language rule: it reads the stripped text (each token replaced by `§`) and the context, and
/// returns `(code, detail)` pairs. The path is added by [`validate_text`].
pub type TextRule = Box<dyn Fn(&str, &TextCtx) -> Vec<(ValidationCode, String)> + Send + Sync>;

/// What one paragraph must satisfy.
#[derive(Default)]
pub struct TextRules {
    /// At least one citation (weakness, focus and opening items).
    pub require_citation: bool,
    /// `Some`: `line` tokens only for these keys. `None`: any line the evidence knows.
    pub line_keys: Option<Vec<String>>,
    /// Position tokens are their own citations (chat), so the citation scoping is skipped.
    pub implicit_citations: bool,
    pub extra: Vec<TextRule>,
}

fn err(code: ValidationCode, path: &str, detail: impl Into<String>) -> ValidationError {
    ValidationError {
        code,
        path: path.to_owned(),
        detail: detail.into(),
    }
}

fn key(game_id: &str, ply: u32) -> String {
    format!("{game_id}#{ply}")
}

/// The cited position of a `move`/`best`/`pv` token.
fn position_of(t: &TokenBody) -> Option<String> {
    match t {
        TokenBody::Move { game_id, ply }
        | TokenBody::Best { game_id, ply }
        | TokenBody::Pv { game_id, ply, .. } => Some(key(game_id, *ply)),
        _ => None,
    }
}

/// JSON-quoted, so a fragment can never break the detail format.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn code_rank(c: ValidationCode) -> usize {
    ValidationCode::ALL
        .iter()
        .position(|x| *x == c)
        .unwrap_or(usize::MAX)
}

fn sorted(mut errors: Vec<ValidationError>) -> Vec<ValidationError> {
    errors.sort_by(|a, b| {
        (code_rank(a.code), &a.path, &a.detail).cmp(&(code_rank(b.code), &b.path, &b.detail))
    });
    errors
}

/// Stage 4: any move, number or date the extractors find in the stripped text.
fn extracted_facts(stripped: &str, path: &str) -> Vec<ValidationError> {
    use ValidationCode as C;
    let mut errors = Vec::new();
    let moves = extract_moves(stripped);
    for m in moves.san.iter().chain(&moves.uci) {
        errors.push(err(C::UnknownMove, path, quoted(m)));
    }
    for n in extract_numbers(stripped) {
        errors.push(err(C::UnknownNumber, path, n.value.to_string()));
    }
    for d in extract_dates(stripped) {
        let year = d.year.map_or_else(|| "?".to_owned(), |y| y.to_string());
        errors.push(err(
            C::UnknownDate,
            path,
            format!("{year}-{:02}-{:02}", d.month, d.day),
        ));
    }
    errors
}

/// An evaluation path in pawns (`….evalAfter.pawns`, `….avgEvalAtOpeningEnd.pawns`).
pub fn is_eval_pawns_path(path: &str) -> bool {
    let mut it = path.rsplit('.');
    it.next() == Some("pawns")
        && it
            .next()
            .is_some_and(|s| s.to_ascii_lowercase().contains("eval"))
}

/// Whether a `num` token may show `value` from `path` in `fmt` (fix round 1, C2):
/// - `pct0`/`pct1` only for a rate or share path ([`Evidence::rate_paths`]) with a value in
///   [0, 1];
/// - `int` and no format only for whole numbers;
/// - `signed1` only for an evaluation in pawns;
/// - `dec1`/`dec2` for any value.
pub fn num_format_allowed(ev: &Evidence, path: &str, value: f64, fmt: Option<NumFormat>) -> bool {
    match fmt {
        Some(NumFormat::Pct0 | NumFormat::Pct1) => {
            ev.rate_paths.contains(path) && (0.0..=1.0).contains(&value)
        }
        None | Some(NumFormat::Int) => value.fract() == 0.0,
        Some(NumFormat::Signed1) => is_eval_pawns_path(path),
        Some(NumFormat::Dec1 | NumFormat::Dec2) => true,
    }
}

/// Characters that glue a token into notation on either side (`Qx{{sq}}`, `{{num}}%`, `-{{num}}`).
fn is_glue(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(c, '#' | '+' | '=' | '/' | '%' | '\u{BD}' | '\u{D7}' | '-')
        || is_dash(c)
}

fn skippable(c: char) -> bool {
    is_invisible(c) || is_combining_mark(c)
}

/// What sits next to a token: nothing, another token, or a char (after skipping invisibles and
/// combining marks), plus what is beyond that char (`TOKEN_MARK` for a token).
enum Side {
    Edge,
    Token,
    Char(char, Option<char>),
}

/// Stands for "a token" beyond a `.` or `,` (no `{` survives the syntax barrier).
const TOKEN_MARK: char = '{';

fn side_before(text: &str, at: usize, token_ends: &HashSet<usize>) -> Side {
    let mut pos = at;
    loop {
        if token_ends.contains(&pos) {
            return Side::Token;
        }
        let Some(c) = text[..pos].chars().next_back() else {
            return Side::Edge;
        };
        count_scanned(c.len_utf8());
        pos -= c.len_utf8();
        if skippable(c) {
            continue;
        }
        let beyond = if token_ends.contains(&pos) {
            Some(TOKEN_MARK)
        } else {
            let mut b = None;
            for x in text[..pos].chars().rev() {
                count_scanned(x.len_utf8());
                if !skippable(x) {
                    b = Some(x);
                    break;
                }
            }
            b
        };
        return Side::Char(c, beyond);
    }
}

fn side_after(text: &str, at: usize, token_starts: &HashSet<usize>) -> Side {
    let mut pos = at;
    loop {
        if token_starts.contains(&pos) {
            return Side::Token;
        }
        let Some(c) = text[pos..].chars().next() else {
            return Side::Edge;
        };
        count_scanned(c.len_utf8());
        pos += c.len_utf8();
        if skippable(c) {
            continue;
        }
        let beyond = if token_starts.contains(&pos) {
            Some(TOKEN_MARK)
        } else {
            let mut b = None;
            for x in text[pos..].chars() {
                count_scanned(x.len_utf8());
                if !skippable(x) {
                    b = Some(x);
                    break;
                }
            }
            b
        };
        return Side::Char(c, beyond);
    }
}

fn glues(side: &Side) -> bool {
    match side {
        Side::Edge => false,
        Side::Token => true,
        Side::Char(c, beyond) => {
            is_glue(*c)
                || (matches!(c, '.' | ',')
                    && beyond.is_some_and(|b| b == TOKEN_MARK || b.is_alphanumeric()))
        }
    }
}

/// The tokens glued to their neighbours (fix round 1, C1): a token directly next to a letter, a
/// digit, one of `x # + = - / % ½ ×` or a dash, or another token, is rejected, and so is a `.`
/// or `,` between a token and a letter, a digit or another token (`{{num}}.{{num}}`,
/// `{{num}},5`). Invisible characters and combining marks between them do not separate them.
/// Sentence punctuation (`{{move}}.` followed by a space) is fine.
pub fn glued(canonical: &str, tokens: &[Token]) -> Vec<usize> {
    let starts: HashSet<usize> = tokens.iter().map(|t| t.index).collect();
    let ends: HashSet<usize> = tokens.iter().map(|t| t.index + t.raw.len()).collect();
    let mut out = Vec::new();
    for (k, t) in tokens.iter().enumerate() {
        let before = side_before(canonical, t.index, &ends);
        let after = side_after(canonical, t.index + t.raw.len(), &starts);
        if glues(&before) || glues(&after) {
            out.push(k);
        }
    }
    out
}

/// The squares of a position's recorded moves and engine lines (UCI from/to, SAN targets).
fn position_squares(ev: &Evidence, key: &str, out: &mut HashSet<String>) {
    let Some(p) = ev.positions.get(key) else {
        return;
    };
    for u in &p.allowed_uci {
        for r in [0..2, 2..4] {
            if let Some(sq) = u.get(r) {
                out.insert(sq.to_owned());
            }
        }
    }
    for san in &p.allowed_san {
        let b = san.as_bytes();
        for w in b.windows(2) {
            if (b'a'..=b'h').contains(&w[0]) && (b'1'..=b'8').contains(&w[1]) {
                out.insert(String::from_utf8_lossy(w).into_owned());
            }
        }
    }
}

/// Validates one paragraph. `Ok` carries the rendered segments; `Err` every error found, sorted
/// (see the module docs), each with `path`.
pub fn validate_text(
    text: &str,
    citations: &[Citation],
    ev: &Evidence,
    rules: &TextRules,
    path: &str,
) -> Result<Vec<Segment>, Vec<ValidationError>> {
    debug_assert!(
        text.chars().count() <= ITEM_TEXT_MAX_CHARS,
        "validate_text takes schema-checked text"
    );
    validate(text, citations, ev, rules, path)
}

fn validate(
    text: &str,
    citations: &[Citation],
    ev: &Evidence,
    rules: &TextRules,
    path: &str,
) -> Result<Vec<Segment>, Vec<ValidationError>> {
    use ValidationCode as C;
    let mut errors = Vec::new();

    // 1. Citations.
    for c in citations {
        let k = key(&c.game_id, c.ply);
        if !ev.positions.contains_key(&k) {
            errors.push(err(C::UnknownCitation, path, k));
        }
    }
    if rules.require_citation && citations.is_empty() {
        errors.push(err(C::MissingCitation, path, "no citation"));
    }

    // 2. Token syntax: the only hard barrier. Token-error indexes are in the canonical text and
    // stray-brace indexes in `stripped`, so neither goes into the detail.
    let parsed = parse_tokens(text);
    let mut syntax: Vec<ValidationError> = parsed
        .errors
        .iter()
        .map(|e| {
            err(
                C::TokenSyntax,
                path,
                format!("{}: {}", e.message, quoted(&e.raw)),
            )
        })
        .collect();
    let whole = parsed.tokens.len() == 1
        && parsed.stripped.trim() == super::tokens::TOKEN_PLACEHOLDER.to_string();
    for t in &parsed.tokens {
        if t.body == TokenBody::Offtopic && !whole {
            syntax.push(err(
                C::TokenSyntax,
                path,
                "{{offtopic}} must be the whole paragraph",
            ));
        }
    }
    if !syntax.is_empty() {
        errors.extend(syntax);
        return Err(sorted(errors));
    }
    let stripped = parsed.stripped.as_str();
    let canonical = canonical_for_tokens(text);

    // 3. Leaks, and 4. defence in depth when stage 3 found nothing.
    let leaks: Vec<_> = find_leaks(stripped)
        .into_iter()
        .filter(|l| !is_discourse_ordinal(stripped, l))
        .collect();
    if leaks.is_empty() {
        errors.extend(extracted_facts(stripped, path));
    } else {
        let detail = leaks
            .iter()
            .map(|l| format!("{}:{}", l.kind.as_str(), quoted(&l.text)))
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(err(C::FreeTextFact, path, detail));
    }

    // 5. Tokens resolve, formats suit values, and nothing is glued.
    for t in &parsed.tokens {
        if resolve_token(ev, &t.body).is_none() {
            errors.push(err(C::UnresolvedToken, path, quoted(&t.raw)));
        } else if let TokenBody::Num { path: p, fmt } = &t.body
            && let Some(v) = ev.number_paths.get(p)
            && !num_format_allowed(ev, p, *v, *fmt)
        {
            errors.push(err(
                C::UnresolvedToken,
                path,
                format!("{}: format not allowed for this value", quoted(&t.raw)),
            ));
        }
    }
    for k in glued(&canonical, &parsed.tokens) {
        errors.push(err(C::GluedToken, path, quoted(&parsed.tokens[k].raw)));
    }

    // 6. Scoping to the cited positions.
    let mut cited: HashSet<String> = citations.iter().map(|c| key(&c.game_id, c.ply)).collect();
    if rules.implicit_citations {
        cited.extend(parsed.tokens.iter().filter_map(|t| position_of(&t.body)));
    } else {
        for t in &parsed.tokens {
            if position_of(&t.body).is_some_and(|k| !cited.contains(&k)) {
                errors.push(err(C::UncitedReference, path, quoted(&t.raw)));
            }
        }
    }
    if parsed
        .tokens
        .iter()
        .any(|t| matches!(t.body, TokenBody::Sq { .. }))
    {
        let mut squares = HashSet::new();
        for k in &cited {
            position_squares(ev, k, &mut squares);
        }
        for t in &parsed.tokens {
            if let TokenBody::Sq { square } = &t.body
                && !squares.contains(square)
            {
                errors.push(err(
                    C::UncitedReference,
                    path,
                    format!("{}: not a square of a cited position", quoted(&t.raw)),
                ));
            }
        }
    }

    // 7. Line tokens are scoped to the item's lines.
    if let Some(keys) = &rules.line_keys {
        for t in &parsed.tokens {
            if matches!(&t.body, TokenBody::Line { line_key, .. } if !keys.contains(line_key)) {
                errors.push(err(C::UnknownLine, path, quoted(&t.raw)));
            }
        }
    }

    // 8. Language rules, on one normalisation of the paragraph.
    count_scanned(stripped.len());
    let hardened = normalize_text(stripped);
    let ctx = TextCtx {
        tokens: &parsed.tokens,
        citations,
        evidence: ev,
        canonical: &canonical,
        hardened: &hardened,
    };
    for rule in &rules.extra {
        errors.extend(
            rule(stripped, &ctx)
                .into_iter()
                .map(|(code, detail)| err(code, path, detail)),
        );
    }

    if !errors.is_empty() {
        return Err(sorted(errors));
    }
    match render_segments(&canonical, &parsed.tokens, ev) {
        Some(segments) => Ok(segments),
        None => Err(vec![err(
            C::UnresolvedToken,
            path,
            "the text does not render",
        )]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::bounded::take_scanned;
    use crate::validator::language::{
        banned_phrases, engine_claims, no_causal_claims, no_marketing, no_quantity_words,
    };

    /// Stage 4 on its own: in every fixture paragraph the leak detector (stage 3) already
    /// rejects these, so the stage is only reachable through a leak-detector gap.
    #[test]
    fn extracted_facts_flag_every_extracted_move_number_and_date() {
        use ValidationCode as C;
        let codes = |s: &str| {
            extracted_facts(s, "p")
                .into_iter()
                .map(|e| e.code)
                .collect::<Vec<_>>()
        };
        assert_eq!(codes("you lose half the games"), [C::UnknownNumber]);
        assert_eq!(codes("you played h4 too early"), [C::UnknownMove]);
        assert_eq!(codes("the engine line g1f3"), [C::UnknownMove]);
        assert!(codes("since 14 Oct").contains(&C::UnknownDate));
        assert_eq!(codes("you tend to rush \u{a7} in sharp positions"), []);
    }

    #[test]
    fn eval_pawns_paths() {
        assert!(is_eval_pawns_path(
            "weaknesses.W1.exemplars.0.evalAfter.pawns"
        ));
        assert!(is_eval_pawns_path("openings.x.avgEvalAtOpeningEnd.pawns"));
        assert!(!is_eval_pawns_path(
            "weaknesses.W1.exemplars.0.evalAfter.cp"
        ));
        assert!(!is_eval_pawns_path("basis.analysedGames"));
    }

    /// Linear in the text (the scan counter of ADR 0058): every stage, the language rules
    /// included, on hostile shapes.
    #[test]
    fn scanned_bytes_are_linear_on_hostile_shapes() {
        let n = 32 * 1024;
        let shapes: [(&str, String); 9] = [
            ("words", "may well ".repeat(n / 9)),
            ("braces", "{{sq:e4}} {".repeat(n / 11)),
            ("open tokens", "{{".repeat(n / 2)),
            ("glued tokens", "{{sq:e4}}".repeat(n / 9)),
            (
                "invisible runs",
                "{{sq:e4}}\u{180F}\u{180F}\u{180F}\u{180F}".repeat(n / 21),
            ),
            ("will", "will will gain ".repeat(n / 15)),
            ("guarantee", "guaranteeguarantee ".repeat(n / 19)),
            ("reason", "the reason the reason ".repeat(n / 22)),
            ("best blunder", "{{best:g:1}} blunder ".repeat(n / 21)),
        ];
        let rules = TextRules {
            extra: vec![
                banned_phrases(),
                no_marketing(),
                no_causal_claims(),
                engine_claims(),
                no_quantity_words(),
            ],
            ..TextRules::default()
        };
        let ev = Evidence::default();
        for (name, s) in shapes {
            take_scanned();
            let _ = validate(&s, &[], &ev, &rules, "p");
            let used = take_scanned();
            assert!(
                used <= 200 * s.len(),
                "{name}: validate read {used} bytes for {}",
                s.len()
            );
            // The language rules alone, which always run when there is no syntax error.
            let p = parse_tokens(&s);
            let canonical = canonical_for_tokens(&s);
            let hardened = normalize_text(&p.stripped);
            take_scanned();
            let ctx = TextCtx {
                tokens: &p.tokens,
                citations: &[],
                evidence: &ev,
                canonical: &canonical,
                hardened: &hardened,
            };
            for r in &rules.extra {
                let _ = r(&p.stripped, &ctx);
            }
            let used = take_scanned();
            assert!(
                used <= 200 * s.len(),
                "{name}: {used} bytes for {}",
                s.len()
            );
        }
    }
}
