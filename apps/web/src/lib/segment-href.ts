import type { Segment } from "../server/coaching/types";

/**
 * Where a segment links. `move`, `best` and `pv` carry the ply of the move played (Rust
 * `segments.rs`), and M5a's review at `?ply=N` shows the position after ply N with that move
 * marked. `line`, `eng` and the rest link nowhere (the opening explorer is out of M5b's scope).
 */
export function segmentHref(s: Segment): string | null {
  switch (s.kind) {
    case "move":
    case "best":
    case "pv":
      return `/app/games/${encodeURIComponent(s.gameId)}?ply=${s.ply}`;
    default:
      return null;
  }
}
