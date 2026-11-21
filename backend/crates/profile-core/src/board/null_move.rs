//! Null-move FENs for the threat probe (R2 Task 4).

use chess_core::board::Position;

/// The position with the side to move passing: the side flips, en passant is cleared and both
/// clocks are kept. `None` when the side to move is in check (the pass would be illegal), when
/// `fen` is invalid, or when `Position::from_fen` rejects the result. The result is in chess.js
/// FEN form.
pub fn null_move_fen(fen: &str) -> Option<String> {
    let pos = Position::from_fen(fen).ok()?;
    if pos.is_check() {
        return None;
    }
    let fields: Vec<&str> = fen.split_whitespace().collect();
    let [placement, side, castling, _ep, halfmove, fullmove] = fields.as_slice() else {
        return None;
    };
    let side = match *side {
        "w" => "b",
        "b" => "w",
        _ => return None,
    };
    let passed = format!("{placement} {side} {castling} - {halfmove} {fullmove}");
    Position::from_fen(&passed).ok().map(|p| p.fen())
}
