/** Position key: piece placement, side to move, castling rights and en passant (no clocks). */
export function fenKey(fen: string): string {
  return fen.split(" ").slice(0, 4).join(" ");
}
