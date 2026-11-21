//! Playing out PV lines (R2 Task 4).

use chess_core::Color;
use chess_core::board::Position;
use cozy_chess as cc;

use super::{parse_square, piece_char};

/// One played ply of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineStep {
    /// Canonical UCI (castling as the king's two-square step, lowercase promotion).
    pub uci: String,
    /// chess.js SAN with its check or mate suffix.
    pub san: String,
    pub fen_before: String,
    pub fen_after: String,
    pub mover: Color,
    /// Lowercase letter of the captured piece (`'p'` for en passant).
    pub captured: Option<char>,
    /// The move gives check (also true for mate).
    pub is_check: bool,
    pub is_mate: bool,
    /// Lowercase promotion piece.
    pub promotion: Option<char>,
    pub from: String,
    /// Destination square (the king's for castling).
    pub to: String,
}

/// Play `pv` from `fen`, at most `max_plies` moves. Stops at the first illegal or malformed move
/// and never panics; an invalid FEN gives an empty line.
pub fn play_line(fen: &str, pv: &[String], max_plies: Option<usize>) -> Vec<LineStep> {
    let Ok(mut pos) = Position::from_fen(fen) else {
        return Vec::new();
    };
    let mut fen_before = pos.fen();
    let mut steps = Vec::new();
    for uci in pv.iter().take(max_plies.unwrap_or(usize::MAX)) {
        let mover = pos.side_to_move();
        let board_before = pos.board().clone();
        let Ok(played) = pos.push_uci(uci) else {
            break;
        };
        let from = played.uci.get(0..2).unwrap_or_default().to_owned();
        let to = played.uci.get(2..4).unwrap_or_default().to_owned();
        let fen_after = pos.fen();
        steps.push(LineStep {
            captured: captured_piece(&board_before, &from, &to),
            promotion: played.uci.chars().nth(4),
            uci: played.uci,
            san: played.san,
            fen_before: std::mem::replace(&mut fen_before, fen_after.clone()),
            fen_after,
            mover,
            is_check: pos.is_check(),
            is_mate: pos.is_checkmate(),
            from,
            to,
        });
    }
    steps
}

/// The piece a move from `from` to `to` captures on `before`, including en passant.
fn captured_piece(before: &cc::Board, from: &str, to: &str) -> Option<char> {
    let (from, to) = (parse_square(from)?, parse_square(to)?);
    let mover = before.color_on(from)?;
    match (before.piece_on(to), before.color_on(to)) {
        (Some(piece), Some(color)) if color != mover => Some(piece_char(piece)),
        (None, _) if before.piece_on(from) == Some(cc::Piece::Pawn) && from.file() != to.file() => {
            Some('p')
        }
        _ => None,
    }
}
