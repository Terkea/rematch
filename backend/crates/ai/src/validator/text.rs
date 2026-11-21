//! The claim validator for one paragraph of token text (ADR-R3-4 "Claim validator"). It decides
//! whether a paragraph Claude wrote is safe to show and, when it is, renders its segments.
//!
//! The checks run in a fixed order, as stages. The first stage that finds anything ends the
//! check, and its errors are the result: a later stage would only restate an earlier failure
//! (a free-text `Qxf7#` is a leak and also an extracted move). Any error rejects the paragraph,
//! so stopping early never lets anything through.
//!
//! 1. Citations: each is a key of `positions` (`unknown_citation`); `require_citation` with none
//!    gives `missing_citation`.
//! 2. Token syntax: any [`ParsedTokens::errors`] entry, and `{{offtopic}}` anywhere but as the
//!    whole paragraph (`token_syntax`). Prose inside a malformed `{{…}}` is never leak-scanned,
//!    so a malformed token always rejects.
//! 3. Leaks: [`find_leaks`] on the stripped text (`free_text_fact`). It is the TS/hardened union.
//! 4. Defence in depth: [`extract_moves`], [`extract_numbers`] and [`extract_dates`] on the
//!    stripped text (`unknown_move`, `unknown_number`, `unknown_date`). Facts are by reference,
//!    so any result at all is a fact stated outside a token. The extractors are unions too;
//!    `normalize_text` alone (hardened only) is never used for a check.
//! 5. Every token resolves (`unresolved_token`), and the whole paragraph renders.
//! 6. Unless `implicit_citations`, every `move`/`best`/`pv` token's `gameId#ply` is one of the
//!    item's citations (`uncited_reference`).
//! 7. `line` tokens only for keys in `line_keys`, when it is `Some` (`unknown_line`).
//! 8. The `extra` rules (language rules), on the stripped text.

use std::collections::HashSet;

use super::evidence::Evidence;
use super::extract::{extract_dates, extract_moves, extract_numbers};
use super::format::resolve_token;
use super::leaks::find_leaks;
use super::segments_render::render_segments;
use super::tokens::{Token, TokenBody, canonical_for_tokens, parse_tokens};
use crate::output::codes::{Citation, ValidationCode, ValidationError};
use crate::segments::Segment;

/// What a rule sees besides the stripped text.
pub struct TextCtx<'a> {
    pub tokens: &'a [Token],
    pub citations: &'a [Citation],
    pub evidence: &'a Evidence,
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
    /// Position tokens are their own citations (chat), so check 6 is skipped.
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

/// Validates one paragraph. `Ok` carries the rendered segments; `Err` the errors of the first
/// failing stage (see the module docs), each with `path`.
pub fn validate_text(
    text: &str,
    citations: &[Citation],
    ev: &Evidence,
    rules: &TextRules,
    path: &str,
) -> Result<Vec<Segment>, Vec<ValidationError>> {
    use ValidationCode as C;
    let fail = |errors: Vec<ValidationError>| -> Result<(), Vec<ValidationError>> {
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    };

    // 1. Citations.
    let mut errors = Vec::new();
    for c in citations {
        let k = key(&c.game_id, c.ply);
        if !ev.positions.contains_key(&k) {
            errors.push(err(C::UnknownCitation, path, k));
        }
    }
    if rules.require_citation && citations.is_empty() {
        errors.push(err(C::MissingCitation, path, "no citation"));
    }
    fail(errors)?;

    // 2. Token syntax. Token-error indexes are in the canonical text and stray-brace indexes in
    // `stripped`, so neither goes into the detail.
    let parsed = parse_tokens(text);
    let mut errors: Vec<ValidationError> = parsed
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
            errors.push(err(
                C::TokenSyntax,
                path,
                "{{offtopic}} must be the whole paragraph",
            ));
        }
    }
    fail(errors)?;
    let stripped = parsed.stripped.as_str();

    // 3. Leaks.
    let leaks = find_leaks(stripped);
    if !leaks.is_empty() {
        let detail = leaks
            .iter()
            .map(|l| format!("{}:{}", l.kind.as_str(), quoted(&l.text)))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(vec![err(C::FreeTextFact, path, detail)]);
    }

    // 4. Defence in depth.
    fail(extracted_facts(stripped, path))?;

    // 5. Every token resolves, and the paragraph renders.
    let errors: Vec<ValidationError> = parsed
        .tokens
        .iter()
        .filter(|t| resolve_token(ev, &t.body).is_none())
        .map(|t| err(C::UnresolvedToken, path, quoted(&t.raw)))
        .collect();
    fail(errors)?;
    let canonical = canonical_for_tokens(text);
    let Some(segments) = render_segments(&canonical, &parsed.tokens, ev) else {
        return Err(vec![err(
            C::UnresolvedToken,
            path,
            "the text does not render",
        )]);
    };

    // 6. Position tokens are scoped to the item's citations.
    if !rules.implicit_citations {
        let cited: HashSet<String> = citations.iter().map(|c| key(&c.game_id, c.ply)).collect();
        let errors: Vec<ValidationError> = parsed
            .tokens
            .iter()
            .filter(|t| position_of(&t.body).is_some_and(|k| !cited.contains(&k)))
            .map(|t| err(C::UncitedReference, path, quoted(&t.raw)))
            .collect();
        fail(errors)?;
    }

    // 7. Line tokens are scoped to the item's lines.
    if let Some(keys) = &rules.line_keys {
        let errors: Vec<ValidationError> = parsed
            .tokens
            .iter()
            .filter(
                |t| matches!(&t.body, TokenBody::Line { line_key, .. } if !keys.contains(line_key)),
            )
            .map(|t| err(C::UnknownLine, path, quoted(&t.raw)))
            .collect();
        fail(errors)?;
    }

    // 8. Language rules.
    let ctx = TextCtx {
        tokens: &parsed.tokens,
        citations,
        evidence: ev,
    };
    let errors: Vec<ValidationError> = rules
        .extra
        .iter()
        .flat_map(|rule| rule(stripped, &ctx))
        .map(|(code, detail)| err(code, path, detail))
        .collect();
    fail(errors)?;
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::bounded::take_scanned;
    use crate::validator::language::{banned_phrases, no_causal_claims, no_marketing};

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

    /// Linear in the text (the scan counter of ADR-R3-2): every stage, the language rules
    /// included, on hostile shapes.
    #[test]
    fn scanned_bytes_are_linear_on_hostile_shapes() {
        let n = 32 * 1024;
        let shapes: [(&str, String); 6] = [
            ("words", "may well ".repeat(n / 9)),
            ("braces", "{{sq:e4}} {".repeat(n / 11)),
            ("open tokens", "{{".repeat(n / 2)),
            ("will", "will will gain ".repeat(n / 15)),
            ("guarantee", "guaranteeguarantee ".repeat(n / 19)),
            ("reason", "the reason the reason ".repeat(n / 22)),
        ];
        let rules = TextRules {
            extra: vec![banned_phrases(), no_marketing(), no_causal_claims()],
            ..TextRules::default()
        };
        let ev = Evidence::default();
        for (name, s) in shapes {
            take_scanned();
            let _ = validate_text(&s, &[], &ev, &rules, "p");
            // The language rules alone, which run only when the earlier stages pass.
            let ctx = TextCtx {
                tokens: &[],
                citations: &[],
                evidence: &ev,
            };
            for r in &rules.extra {
                let _ = r(&s, &ctx);
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
