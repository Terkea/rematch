//! Board utilities, material swings and the null move (R2 Task 4).
//!
//! Our own code from the documented rules in TS M2 Task 3 and improvement-engine section 4.4, on
//! top of `chess_core::board::Position` and cozy-chess. Every function takes FEN strings and
//! never panics: invalid input yields an empty, `None` or zero result.

pub mod attacks;
pub mod line;
pub mod material;
pub mod null_move;

pub use attacks::{attackers_of, defenders_of, is_en_prise, piece_at, pinned_to};
pub use line::{LineStep, play_line};
pub use material::{
    MATE_GAIN, PIECE_VALUE, SettledPly, material_balance, material_signature, piece_count,
    piece_value, settled_plies, settled_swing,
};
pub use null_move::null_move_fen;

use chess_core::Color;
use cozy_chess as cc;

pub(crate) fn to_cozy(c: Color) -> cc::Color {
    match c {
        Color::White => cc::Color::White,
        Color::Black => cc::Color::Black,
    }
}

/// Lowercase piece letter (`p n b r q k`).
pub(crate) fn piece_char(p: cc::Piece) -> char {
    match p {
        cc::Piece::Pawn => 'p',
        cc::Piece::Knight => 'n',
        cc::Piece::Bishop => 'b',
        cc::Piece::Rook => 'r',
        cc::Piece::Queen => 'q',
        cc::Piece::King => 'k',
    }
}

/// Algebraic name of a square (`e4`).
pub(crate) fn square_name(s: cc::Square) -> String {
    let file = (b'a' + s.file() as u8) as char;
    let rank = (b'1' + s.rank() as u8) as char;
    format!("{file}{rank}")
}

/// Parse an algebraic square name; `None` unless it is exactly `[a-h][1-8]`.
pub(crate) fn parse_square(s: &str) -> Option<cc::Square> {
    match s.as_bytes() {
        [f @ b'a'..=b'h', r @ b'1'..=b'8'] => Some(cc::Square::new(
            cc::File::index(usize::from(f - b'a')),
            cc::Rank::index(usize::from(r - b'1')),
        )),
        _ => None,
    }
}
