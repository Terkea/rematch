/// Position key: piece placement, side to move, castling rights and en passant (no clocks).
pub fn fen_key(fen: &str) -> String {
    fen.split(' ').take(4).collect::<Vec<_>>().join(" ")
}
