import type { PositionLine } from "@ci/ui/position-explorer";

/**
 * The position explorer's `loadLine` (feat/position-explorer): the game around `ply` of one of the
 * user's games (`games.id`) and the engine's stored line, or null when the game is not theirs or
 * the ply not in it. A network or server failure throws, and the explorer says so.
 */
export async function fetchPositionLine(gameId: string, ply: number): Promise<PositionLine | null> {
  const res = await fetch(`/api/games/${encodeURIComponent(gameId)}/line?ply=${ply}`, {
    credentials: "same-origin",
    headers: { accept: "application/json" },
  });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`line ${res.status}`);
  return (await res.json()) as PositionLine;
}
