//! PGN parser with clock comments. Our own tokenizer (no pgn-reader or
//! shakmaty, which are GPL); moves are played with [`Position::play_san`] so
//! SAN, UCI and FEN match chess.js 1.4.0.
//!
//! Behaviour follows `packages/chess-core/src/pgn.ts`, but where the TS code
//! fails soft (nested variations, parentheses inside comments, SAN glued to
//! a comment) this parser handles the input properly.

use crate::Color;
use crate::board::Position;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Errors from [`parse_pgn`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PgnError {
    /// A variant or custom-start game. The message is the full user-facing text.
    #[error("{0}")]
    UnsupportedVariant(String),
    /// `ply` is the 1-based ply of the move that could not be played.
    #[error("illegal move {token:?} at ply {ply}")]
    IllegalMove { ply: u32, token: String },
    #[error("malformed PGN: {0}")]
    Malformed(String),
}

impl PgnError {
    pub fn is_unsupported(&self) -> bool {
        matches!(self, PgnError::UnsupportedVariant(_))
    }
}

/// How a game ended on the board (not the Result tag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Checkmate,
    Stalemate,
    Draw,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ply {
    /// 1-based ply number.
    pub ply: u32,
    pub color: Color,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub fen_after: String,
    /// The mover's clock after the move, in seconds.
    pub clock_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedGame {
    pub headers: HashMap<String, String>,
    pub start_fen: String,
    pub plies: Vec<Ply>,
    pub terminal: Option<Terminal>,
}

impl ParsedGame {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

fn header_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"^\s*\[(\w+)\s+"((?:[^"\\]|\\.)*)"\]\s*$"#).expect("regex"))
}

fn clock_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\[%clk (\d+):(\d+):(\d+(?:\.\d+)?)\]").expect("regex"))
}

fn move_number_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\d+\.+").expect("regex"))
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n @ ('"' | '\\')) => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn split_pgn(pgn: &str) -> (HashMap<String, String>, String) {
    let mut headers = HashMap::new();
    let mut body: Vec<&str> = Vec::new();
    let mut in_headers = true;
    for line in pgn.lines() {
        let caps = if in_headers {
            header_re().captures(line)
        } else {
            None
        };
        if let Some(c) = caps {
            headers.insert(c[1].to_string(), unescape(&c[2]));
        } else if in_headers && line.trim().is_empty() {
            // blank lines between or after the header block
        } else {
            in_headers = false;
            body.push(line);
        }
    }
    (headers, body.join("\n"))
}

fn clock_of(comment: &str) -> Option<f64> {
    let c = clock_re().captures(comment)?;
    let h: f64 = c[1].parse().ok()?;
    let m: f64 = c[2].parse().ok()?;
    let s: f64 = c[3].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

fn is_result(tok: &str) -> bool {
    matches!(tok, "1-0" | "0-1" | "1/2-1/2" | "*")
}

fn is_nag(tok: &str) -> bool {
    tok.strip_prefix('$')
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// Movetext tokens: SAN move tokens, each with the clock read from the
/// comments that follow it (the last clock comment wins, as in TS).
fn tokenize(movetext: &str) -> Result<Vec<(String, Option<f64>)>, PgnError> {
    let mut moves: Vec<(String, Option<f64>)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    let bytes = movetext.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'{' => {
                let end = movetext[i + 1..]
                    .find('}')
                    .ok_or_else(|| PgnError::Malformed("unterminated comment".into()))?;
                let comment = &movetext[i + 1..i + 1 + end];
                if depth == 0
                    && let Some(secs) = clock_of(comment)
                    && let Some(last) = moves.last_mut()
                {
                    last.1 = Some(secs);
                }
                i += end + 2;
            }
            b';' => {
                i = movetext[i..].find('\n').map_or(bytes.len(), |n| i + n + 1);
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| PgnError::Malformed("unmatched ')'".into()))?;
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'{' | b';' | b'(' | b')')
                {
                    i += 1;
                }
                if depth > 0 {
                    continue;
                }
                let raw = &movetext[start..i];
                if is_result(raw) {
                    break;
                }
                if is_nag(raw) {
                    continue;
                }
                let tok = match move_number_re().find(raw) {
                    Some(m) => &raw[m.end()..],
                    None => raw,
                };
                if tok.is_empty() || tok.bytes().all(|b| b == b'!' || b == b'?') {
                    continue;
                }
                moves.push((tok.to_string(), None));
            }
        }
    }
    if depth > 0 {
        return Err(PgnError::Malformed("unterminated variation".into()));
    }
    Ok(moves)
}

fn terminal_of(pos: &Position, seen: &[String]) -> Option<Terminal> {
    if pos.is_checkmate() {
        Some(Terminal::Checkmate)
    } else if pos.is_stalemate() {
        Some(Terminal::Stalemate)
    } else if pos.halfmove_clock() >= 100
        || pos.is_insufficient_material()
        || seen.iter().filter(|k| **k == pos.repetition_key()).count() >= 3
    {
        Some(Terminal::Draw)
    } else {
        None
    }
}

/// Parse one standard-chess game, rejecting variants and custom starts.
pub fn parse_pgn(pgn: &str) -> Result<ParsedGame, PgnError> {
    let (headers, movetext) = split_pgn(pgn);

    if let Some(variant) = headers.get("Variant") {
        let v = variant.trim().to_lowercase();
        if v != "standard" && v != "chess" {
            return Err(PgnError::UnsupportedVariant(format!(
                "Unsupported variant: {variant}"
            )));
        }
    }
    if headers.contains_key("SetUp") || headers.contains_key("FEN") {
        return Err(PgnError::UnsupportedVariant(
            "Games from a custom starting position are not supported".into(),
        ));
    }

    let tokens = tokenize(&movetext)?;
    let mut pos = Position::start();
    let start_fen = pos.fen();
    let mut seen = vec![pos.repetition_key()];
    let mut plies = Vec::with_capacity(tokens.len());
    for (i, (token, clock)) in tokens.iter().enumerate() {
        let ply = i as u32 + 1;
        let color = pos.side_to_move();
        let fen_before = pos.fen();
        let played = pos.play_san(token).map_err(|_| PgnError::IllegalMove {
            ply,
            token: token.clone(),
        })?;
        seen.push(pos.repetition_key());
        plies.push(Ply {
            ply,
            color,
            san: played.san,
            uci: played.uci,
            fen_before,
            fen_after: pos.fen(),
            clock_seconds: *clock,
        });
    }
    let terminal = terminal_of(&pos, &seen);
    Ok(ParsedGame {
        headers,
        start_fen,
        plies,
        terminal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_move_rule_is_a_draw_but_mate_wins() {
        let pos = Position::from_fen("8/8/8/4k3/8/8/4K3/R7 w - - 100 80").unwrap();
        assert_eq!(terminal_of(&pos, &[]), Some(Terminal::Draw));
        let pos = Position::from_fen("8/8/8/4k3/8/8/4K3/R7 w - - 99 80").unwrap();
        assert_eq!(terminal_of(&pos, &[]), None);
        let mate = Position::from_fen("R5k1/5ppp/8/8/8/8/8/6K1 b - - 100 80").unwrap();
        assert_eq!(terminal_of(&mate, &[]), Some(Terminal::Checkmate));
    }

    #[test]
    fn insufficient_material_is_a_draw() {
        let pos = Position::from_fen("8/8/8/4k3/8/8/4K3/6N1 w - - 0 80").unwrap();
        assert_eq!(terminal_of(&pos, &[]), Some(Terminal::Draw));
    }
}
