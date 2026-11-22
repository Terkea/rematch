//! Narrative to segments (ADR-AI-8): text runs and resolved token segments, in order. The web
//! renders segments and never parses tokens itself.

use super::bounded::count_scanned;
use super::evidence::Evidence;
use super::format::{format_num_at, resolve_token};
use super::tokens::{Token, TokenBody};
use crate::segments::Segment;

fn segment(ev: &Evidence, t: &TokenBody) -> Option<Segment> {
    let text = resolve_token(ev, t)?;
    Some(match t {
        TokenBody::Move { game_id, ply } => Segment::Move {
            game_id: game_id.clone(),
            ply: *ply,
            san: text,
        },
        TokenBody::Best { game_id, ply } => Segment::Best {
            game_id: game_id.clone(),
            ply: *ply,
            san: text,
        },
        TokenBody::Pv {
            game_id,
            ply,
            side,
            n,
        } => Segment::Pv {
            game_id: game_id.clone(),
            ply: *ply,
            side: *side,
            n: *n,
            san: text,
        },
        TokenBody::Line { line_key, n } => Segment::Line {
            line_key: line_key.clone(),
            n: *n,
            san: text,
        },
        TokenBody::Num { path, fmt } => {
            let value = *ev.number_paths.get(path)?;
            Segment::Num {
                path: path.clone(),
                value,
                display: format_num_at(ev, path, *fmt)?,
            }
        }
        TokenBody::Date { path } => Segment::Date {
            path: path.clone(),
            date: text,
        },
        TokenBody::Sq { square } => Segment::Square {
            square: square.clone(),
        },
        TokenBody::Eng { eval_id, line, n } => Segment::Eng {
            eval_id: eval_id.clone(),
            line: *line,
            n: *n,
            text,
        },
        TokenBody::Offtopic => return None,
    })
}

/// Renders `canonical` (from [`super::tokens::canonical_for_tokens`]) with its parsed `tokens`
/// (byte offsets into `canonical`, in order). Returns `None` if any token does not resolve, or if
/// a token does not sit at its index in `canonical` (out of order, overlapping, out of range).
/// Empty text runs are left out. Never panics.
pub fn render_segments(canonical: &str, tokens: &[Token], ev: &Evidence) -> Option<Vec<Segment>> {
    count_scanned(canonical.len());
    let mut out = Vec::with_capacity(tokens.len() * 2 + 1);
    let mut pos = 0;
    for t in tokens {
        let end = t.index.checked_add(t.raw.len())?;
        if t.index < pos || canonical.get(t.index..end)? != t.raw {
            return None;
        }
        let text = canonical.get(pos..t.index)?;
        if !text.is_empty() {
            out.push(Segment::Text {
                text: text.to_owned(),
            });
        }
        out.push(segment(ev, &t.body)?);
        pos = end;
    }
    let tail = canonical.get(pos..)?;
    if !tail.is_empty() {
        out.push(Segment::Text {
            text: tail.to_owned(),
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::bounded::take_scanned;
    use crate::validator::tokens::{canonical_for_tokens, parse_tokens};

    /// Linear: rendering reads the canonical text once (the scan counter of ADR 0058).
    #[test]
    fn scanned_bytes_are_linear() {
        let ev = Evidence::default();
        let text = "word {{sq:e4}} ".repeat(4096);
        let canonical = canonical_for_tokens(&text);
        let tokens = parse_tokens(&text).tokens;
        take_scanned();
        let out = render_segments(&canonical, &tokens, &ev).expect("renders");
        assert_eq!(out.len(), 2 * 4096 + 1);
        assert!(take_scanned() <= canonical.len());
    }
}
