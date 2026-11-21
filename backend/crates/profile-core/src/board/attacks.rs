//! Attack and defence queries (R2 Task 4).
//!
//! Attacks are pseudo-legal, as chess.js `attackers`: a piece attacks a square it could capture
//! on if an enemy stood there, sliders are blocked by any piece, and pins and king safety are
//! ignored. Square lists are in chess.js board order (a8, b8 .. h1).

use chess_core::Color;
use chess_core::board::Position;
use cozy_chess::{self as cc, BitBoard, Piece, Square};

use super::material::{placement_pieces, value_of};
use super::{parse_square, square_name, to_cozy};

/// The colour and lowercase letter of the piece on `square`, read from the FEN's placement field.
pub fn piece_at(fen: &str, square: &str) -> Option<(Color, char)> {
    let sq = parse_square(square)?;
    let (file, rank) = (sq.file() as u8, sq.rank() as u8);
    placement_pieces(fen)
        .into_iter()
        .find(|(f, r, _)| (*f, *r) == (file, rank))
        .map(|(_, _, c)| {
            let color = if c.is_ascii_uppercase() {
                Color::White
            } else {
                Color::Black
            };
            (color, c.to_ascii_lowercase())
        })
}

/// Pieces of `color` attacking `sq` on `board`.
fn attackers_bb(board: &cc::Board, sq: Square, color: cc::Color) -> BitBoard {
    let occupied = board.occupied();
    let diagonal = board.pieces(Piece::Bishop) | board.pieces(Piece::Queen);
    let straight = board.pieces(Piece::Rook) | board.pieces(Piece::Queen);
    board.colors(color)
        & ((cc::get_pawn_attacks(sq, !color) & board.pieces(Piece::Pawn))
            | (cc::get_knight_moves(sq) & board.pieces(Piece::Knight))
            | (cc::get_bishop_moves(sq, occupied) & diagonal)
            | (cc::get_rook_moves(sq, occupied) & straight)
            | (cc::get_king_moves(sq) & board.pieces(Piece::King)))
}

/// Square names in chess.js board order (a8 first).
fn names(bb: BitBoard) -> Vec<String> {
    let mut squares: Vec<Square> = bb.iter().collect();
    squares.sort_by_key(|s| (7 - s.rank() as usize, s.file() as usize));
    squares.into_iter().map(square_name).collect()
}

fn attackers(fen: &str, square: &str, color: Color) -> Vec<String> {
    let (Ok(pos), Some(sq)) = (Position::from_fen(fen), parse_square(square)) else {
        return Vec::new();
    };
    names(attackers_bb(pos.board(), sq, to_cozy(color)))
}

/// Squares of the pieces of `color` that attack `square`.
pub fn attackers_of(fen: &str, square: &str, color: Color) -> Vec<String> {
    attackers(fen, square, color)
}

/// Squares of the pieces of `color` that defend `square` (attack it, so they could recapture
/// there). The same query as [`attackers_of`], named for the defending side.
pub fn defenders_of(fen: &str, square: &str, color: Color) -> Vec<String> {
    attackers(fen, square, color)
}

/// The piece on `square` can be won: it is attacked and either undefended or attacked by a
/// cheaper piece (an enemy king never counts against a defended piece). Kings and empty squares
/// are never en prise.
pub fn is_en_prise(fen: &str, square: &str) -> bool {
    let (Ok(pos), Some(sq)) = (Position::from_fen(fen), parse_square(square)) else {
        return false;
    };
    let board = pos.board();
    let (Some(piece), Some(color)) = (board.piece_on(sq), board.color_on(sq)) else {
        return false;
    };
    if piece == Piece::King {
        return false;
    }
    let attackers = attackers_bb(board, sq, !color);
    if attackers.is_empty() {
        return false;
    }
    if attackers_bb(board, sq, color).is_empty() {
        return true;
    }
    attackers.iter().any(|a| {
        board
            .piece_on(a)
            .is_some_and(|p| p != Piece::King && value_of(p) < value_of(piece))
    })
}

/// The eight line directions as (file, rank) steps.
const DIRECTIONS: [(i8, i8); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];

/// The first occupied square from `from` (exclusive) in direction `(df, dr)`.
fn first_piece(board: &cc::Board, from: Square, df: i8, dr: i8) -> Option<Square> {
    let mut sq = from;
    loop {
        sq = sq.try_offset(df, dr)?;
        if board.occupied().has(sq) {
            return Some(sq);
        }
    }
}

/// `Some((pinner, behind))` when the piece on `square` is pinned by an enemy slider (rook or
/// queen along a rank or file, bishop or queen along a diagonal) to its own king or to an own
/// piece worth more than it. A pin to the king is preferred over a relative pin. Kings and empty
/// squares are never pinned.
pub fn pinned_to(fen: &str, square: &str) -> Option<(String, String)> {
    let pos = Position::from_fen(fen).ok()?;
    let board = pos.board();
    let sq = parse_square(square)?;
    let (piece, color) = (board.piece_on(sq)?, board.color_on(sq)?);
    if piece == Piece::King {
        return None;
    }
    let mut relative = None;
    for (df, dr) in DIRECTIONS {
        let Some(behind) = first_piece(board, sq, df, dr) else {
            continue;
        };
        let Some(behind_piece) = board.piece_on(behind) else {
            continue;
        };
        let to_king = behind_piece == Piece::King;
        if board.color_on(behind) != Some(color)
            || !(to_king || value_of(behind_piece) > value_of(piece))
        {
            continue;
        }
        let Some(pinner) = first_piece(board, sq, -df, -dr) else {
            continue;
        };
        let straight = df == 0 || dr == 0;
        let slides_here = match board.piece_on(pinner) {
            Some(Piece::Queen) => true,
            Some(Piece::Rook) => straight,
            Some(Piece::Bishop) => !straight,
            _ => false,
        };
        if board.color_on(pinner) == Some(color) || !slides_here {
            continue;
        }
        let found = (square_name(pinner), square_name(behind));
        if to_king {
            return Some(found);
        }
        relative.get_or_insert(found);
    }
    relative
}
