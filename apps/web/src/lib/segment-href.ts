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

type GameSegment = Extract<Segment, { kind: "move" | "best" | "pv" }>;
const linksToGame = (s: Segment): s is GameSegment =>
  s.kind === "move" || s.kind === "best" || s.kind === "pv";

/** The platform game ids the linking segments name, once each, in order. */
export function segmentGameIds(segments: readonly Segment[] | null | undefined): string[] {
  const out = new Set<string>();
  for (const s of segments ?? [])
    if (linksToGame(s) && typeof s.gameId === "string") out.add(s.gameId);
  return [...out];
}

/**
 * `segmentHref` for chat answers: their segments name a game by its PLATFORM id (the coaching
 * input's id), while the review takes `games.id`. `ids` maps one to the other for the
 * conversation's own account (`reviewGameIds`); a game it does not hold links nowhere, so no
 * answer ever links to a not-found page.
 */
export function reviewHref(ids: Readonly<Record<string, string>>): (s: Segment) => string | null {
  return (s) => {
    if (!linksToGame(s) || !Object.hasOwn(ids, s.gameId)) return null;
    const id = ids[s.gameId];
    return id ? segmentHref({ ...s, gameId: id }) : null;
  };
}
