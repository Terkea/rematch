//! Engine lines as SAN: a port of m3's `validator/pv.ts`.
//!
//! TS plays each UCI move with chess.js `move({from, to, promotion})`. Our board
//! ([`chess_core::board::Position`], chess.js 1.4.0 parity) plays UCI with `push_uci`, which is
//! more permissive in two ways; both are narrowed here to chess.js's behaviour:
//! - chess.js ignores a promotion letter on a move that is not a promotion (`e2e4q` is `e4`);
//! - chess.js has no king-takes-rook castling (`e1h1` is illegal), which `push_uci` accepts.

use chess_core::board::{PlayedMove, Position};

/// TS `UCI = /^([a-h][1-8])([a-h][1-8])([qrbn])?$/`.
fn is_uci(u: &str) -> bool {
    let b = u.as_bytes();
    let square = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    match b {
        [f1, r1, f2, r2] => square(*f1, *r1) && square(*f2, *r2),
        [f1, r1, f2, r2, p] => square(*f1, *r1) && square(*f2, *r2) && b"qrbn".contains(p),
        _ => false,
    }
}

/// chess.js `move({from, to, promotion})`: `None` when it would throw. The position is unchanged
/// then.
fn play(pos: &mut Position, u: &str) -> Option<PlayedMove> {
    if !is_uci(u) {
        return None;
    }
    let exact = |pos: &mut Position, u: &str| {
        let mut next = pos.clone();
        let m = next.push_uci(u).ok()?;
        // `push_uci` reports castling as the king's step; anything else it accepted is a
        // king-takes-rook spelling that chess.js rejects.
        (m.uci == u).then(|| {
            *pos = next;
            m
        })
    };
    exact(pos, u).or_else(|| {
        // A promotion letter on a non-promotion move: chess.js ignores it. (On a real promotion
        // the 4-char form is rejected by `push_uci`, as chess.js rejects a missing letter.)
        if u.len() == 5 {
            exact(pos, &u[..4])
        } else {
            None
        }
    })
}

/// Plays `uci` from `fen`; stops at the first illegal move. Never panics. SAN is as chess.js
/// renders it.
pub fn pv_to_san(fen: &str, uci: &[String]) -> Vec<String> {
    let Ok(mut pos) = Position::from_fen(fen) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for u in uci {
        match play(&mut pos, u) {
            Some(m) => out.push(m.san),
            None => break,
        }
    }
    out
}

/// FEN after playing `uci` from `fen`, or `None` when illegal or the FEN is invalid.
pub fn fen_after(fen: &str, uci: &str) -> Option<String> {
    let mut pos = Position::from_fen(fen).ok()?;
    play(&mut pos, uci)?;
    Some(pos.fen())
}

/// TS `san.replace(/[+#!?]+$/g, "")`: drops trailing check, mate and annotation marks.
pub fn strip_san(san: &str) -> String {
    san.trim_end_matches(['+', '#', '!', '?']).to_owned()
}
