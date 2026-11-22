"use client";

import { PositionExplorer, type PositionExplorerProps } from "@ci/ui/position-explorer";
import { fetchPositionLine } from "../lib/position-line";

/**
 * A position from one of the player's games as a `PositionExplorer` (N14), for server pages:
 * the explorer's `loadLine` is a function, so it is attached here, on the client. `loadable` is
 * true when `gameId` is the review's game id (`games.id`); without it the expanded view has the
 * cited moves only.
 */
export function GamePosition({
  loadable,
  ...props
}: Omit<PositionExplorerProps, "loadLine" | "line"> & { loadable: boolean }) {
  return <PositionExplorer {...props} loadLine={loadable ? fetchPositionLine : undefined} />;
}
