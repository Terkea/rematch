//! Local Syzygy probe pool (R2 Task 23, ADR-R2-12).
//!
//! Stockfish with `SyzygyPath` answers exact WDL for positions of at most 5 pieces. Positions
//! that the tables do not cover (more pieces, castling rights, an invalid FEN, a material
//! signature without a table file) are never probed and answer `None`, never a guess.

pub mod cached;
pub mod pool;
pub mod wdl;

/// Largest piece count (kings included) of the local 3-4-5 set.
pub const MAX_PIECES: u32 = 5;

/// The canonical FEN (chess-core re-serialised, impossible en-passant squares dropped) when the
/// position can be answered by a Syzygy table: valid, at most `max_pieces` pieces, and no
/// castling rights (Syzygy tables have no castling). `None` otherwise.
pub fn covered_fen(fen: &str, max_pieces: u32) -> Option<String> {
    let canon = engine::uci::canonical_fen(fen).ok()?;
    let mut fields = canon.split(' ');
    let placement = fields.next()?;
    let castling = fields.nth(1)?;
    if castling != "-" {
        return None;
    }
    let pieces = placement.chars().filter(char::is_ascii_alphabetic).count();
    (pieces as u32 <= max_pieces).then_some(canon)
}

/// Syzygy table stems for a placement, each side as K then Q, R, B, N, P: `(white, black)`,
/// e.g. `("KQ", "K")`. Table files list either side first (`KQvK`, `KBNvKQ`).
pub(crate) fn material_sides(placement: &str) -> (String, String) {
    let side = |upper: bool| {
        let mut s = String::new();
        for p in ['K', 'Q', 'R', 'B', 'N', 'P'] {
            let want = if upper { p } else { p.to_ascii_lowercase() };
            for _ in placement.chars().filter(|c| *c == want) {
                s.push(p);
            }
        }
        s
    };
    (side(true), side(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_fen_rules() {
        assert_eq!(
            covered_fen("8/8/8/8/8/4k3/8/KQ6 w - - 3 9", 5).as_deref(),
            Some("8/8/8/8/8/4k3/8/KQ6 w - - 3 9")
        );
        assert_eq!(covered_fen("4k3/8/8/8/8/8/8/4K2R w K - 0 1", 5), None);
        assert_eq!(covered_fen("4k3/8/8/8/8/8/PPPP4/4K3 w - - 0 1", 5), None);
        assert_eq!(covered_fen("garbage", 5), None);
    }

    #[test]
    fn material_sides_order() {
        assert_eq!(
            material_sides("7k/8/8/8/8/8/8/KBN4q"),
            ("KBN".to_owned(), "KQ".to_owned())
        );
        assert_eq!(
            material_sides("8/4k3/8/4K3/4P3/8/8/8"),
            ("KP".to_owned(), "K".to_owned())
        );
    }
}
