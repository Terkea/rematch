//! Movetext extraction (R2 Task 19): the main-line SAN tokens of a game, without replaying them.
//!
//! Skipped: the header block, `{…}` comments, `;` comments (to the end of the line), `(…)`
//! variations at any depth (comments inside them are honoured, so a `)` in a comment does not
//! close a variation), NAGs (`$1`), move numbers (`1.`, `1...`, also glued as in `1.e4`) and the
//! trailing `!`/`?` annotations of a move. A game result (`1-0`, `0-1`, `1/2-1/2`, `*`) ends the
//! movetext.
//!
//! Tokens are returned as written, not normalised: 2013 dumps over-disambiguate (`Rd1d2` where
//! chess.js writes `R1d2`). `chess_core::board::Position::play_san` accepts both forms.

/// Byte offset where the movetext starts: after the leading blank and `[…]` header lines.
pub fn movetext_offset(pgn: &str) -> usize {
    let mut offset = 0;
    for line in pgn.split_inclusive('\n') {
        let t = line.trim();
        if t.is_empty() || (t.starts_with('[') && t.ends_with(']')) {
            offset += line.len();
        } else {
            break;
        }
    }
    offset
}

fn is_result(token: &str) -> bool {
    matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*")
}

/// The SAN in `token`, or `None` for a move number or an empty annotation.
fn san_of(token: &str) -> Option<&str> {
    let digits = token.bytes().take_while(u8::is_ascii_digit).count();
    let rest = if digits > 0 && token[digits..].starts_with('.') {
        token[digits..].trim_start_matches('.')
    } else {
        token.trim_start_matches('.')
    };
    let san = rest.trim_end_matches(['!', '?']);
    (!san.is_empty()).then_some(san)
}

/// Index just past the first `end` at or after `i` (or the end of `b`).
fn skip_past(b: &[u8], i: usize, end: u8) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == end)
        .map_or(b.len(), |p| i + p + 1)
}

fn is_delimiter(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b'{' | b'}' | b'(' | b')' | b';' | b'$')
}

/// The first `max_ply` main-line SAN tokens of `pgn` (headers allowed).
pub fn movetext_sans(pgn: &str, max_ply: usize) -> Vec<&str> {
    let text = &pgn[movetext_offset(pgn)..];
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() && out.len() < max_ply {
        match b[i] {
            b'{' => i = skip_past(b, i + 1, b'}'),
            b';' => i = skip_past(b, i + 1, b'\n'),
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'$' => {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            c if c.is_ascii_whitespace() || c == b'}' => i += 1,
            _ => {
                let start = i;
                while i < b.len() && !is_delimiter(b[i]) {
                    i += 1;
                }
                if depth > 0 {
                    continue;
                }
                let token = &text[start..i];
                if is_result(token) {
                    break;
                }
                if let Some(san) = san_of(token) {
                    out.push(san);
                }
            }
        }
    }
    out
}
