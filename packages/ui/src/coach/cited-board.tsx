"use client";

import {
  explorerAnnotations,
  explorerLabel,
  PositionExplorer,
  type PositionLine,
} from "../position-explorer/index.js";
import type { CitedPosition } from "./types.js";

// A position an answer cites (Fix D, N5), shown as a `PositionExplorer` (feat/position-explorer):
// the small board of the position before the move, from the player's side, with the move played
// and the engine's better move, which expands to step through the game, compare the engine's line
// and try moves. Without `loadLine` (or the review's game id) the expanded view has the cited
// moves only.

/** The board's marks: the played move and, when it differs, the engine's better move. */
export const citedAnnotations = (p: CitedPosition) => explorerAnnotations(p);

/** "Position before 30. Rxg7; engine preferred Qe3". */
export const citedLabel = (p: CitedPosition) => explorerLabel({ ...p, label: p.moveText });

export type LoadLine = (gameId: string, ply: number) => Promise<PositionLine | null>;

export function CitedBoard({
  position,
  loadLine,
}: {
  position: CitedPosition;
  loadLine?: LoadLine | undefined;
}) {
  const game = position.reviewGameId;
  return (
    <PositionExplorer
      gameId={game ?? position.gameId}
      ply={position.ply}
      fenBefore={position.fen}
      played={position.played}
      best={position.best}
      orientation={position.orientation}
      label={position.moveText}
      mistake={position.mistake}
      href={position.href}
      loadLine={game ? loadLine : undefined}
    />
  );
}
