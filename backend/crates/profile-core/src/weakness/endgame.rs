//! Endgame type classification (R2 Task 7, rules TS M2 Task 6).
//!
//! The type looks at the non-pawn, non-king pieces of both sides: none -> pawn, only rooks ->
//! rook, only bishops and knights -> minor, only queens -> queen, anything else -> mixed. The
//! tablebase helpers use the same piece count as the enrichment's Syzygy probe (both kings
//! included, at most 5 pieces); nothing here probes a tablebase (Tasks 23 and 25 do).

use chess_core::board::Position;
use cozy_chess::Piece;

use crate::board::{material_signature, piece_count};
use crate::types::EndgameType;

/// Largest piece count (kings included) covered by the local Syzygy 3-4-5 set.
pub const TB_MAX_PIECES: u32 = 5;

/// Material signatures of the basic endgames (W8.basics); every one has at most 5 pieces.
pub const BASIC_SIGNATURES: [&str; 7] =
    ["KPvK", "KQvK", "KRvK", "KRvKP", "KRPvKR", "KQvKP", "KPvKP"];

/// The endgame type of a position, or `None` when the FEN is not a valid position.
pub fn endgame_type(fen: &str) -> Option<EndgameType> {
    let pos = Position::from_fen(fen).ok()?;
    let board = pos.board();
    let has = |p: Piece| !board.pieces(p).is_empty();
    let (q, r, minor) = (
        has(Piece::Queen),
        has(Piece::Rook),
        has(Piece::Bishop) || has(Piece::Knight),
    );
    Some(match (q, r, minor) {
        (false, false, false) => EndgameType::Pawn,
        (false, true, false) => EndgameType::Rook,
        (false, false, true) => EndgameType::Minor,
        (true, false, false) => EndgameType::Queen,
        _ => EndgameType::Mixed,
    })
}

/// Whether the position has at most [`TB_MAX_PIECES`] pieces; `None` for an invalid FEN.
pub fn is_tablebase_eligible(fen: &str) -> Option<bool> {
    Position::from_fen(fen).ok()?;
    Some(piece_count(fen) <= TB_MAX_PIECES)
}

/// Whether a material signature (as [`material_signature`] writes it) is a basic endgame.
pub fn is_basic_signature(signature: &str) -> bool {
    BASIC_SIGNATURES.contains(&signature)
}

/// Whether a valid, tablebase-sized position is a basic endgame; `None` for an invalid FEN.
pub fn is_basic_endgame(fen: &str) -> Option<bool> {
    let eligible = is_tablebase_eligible(fen)?;
    Some(eligible && is_basic_signature(&material_signature(fen)))
}
