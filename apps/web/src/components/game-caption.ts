import { dayText } from "@ci/ui/charts";
import type { GameRow } from "../server/games/types";

/** "71.3%": accuracy to one place. */
export const accuracyText = (accuracy: number) => `${(Math.round(accuracy * 10) / 10).toFixed(1)}%`;

/**
 * One game in a line, for links and captions: "12 Nov 2026, win v Opponent (1520), accuracy
 * 71.3%". The rating is left out when unknown, the accuracy when the game has none.
 */
export function gameCaption(row: GameRow): string {
  const rating = row.opponent.rating === null ? "" : ` (${row.opponent.rating})`;
  const acc = row.accuracy === null ? "" : `, accuracy ${accuracyText(row.accuracy)}`;
  return `${dayText(row.playedAt)}, ${row.result} v ${row.opponent.name}${rating}${acc}`;
}
