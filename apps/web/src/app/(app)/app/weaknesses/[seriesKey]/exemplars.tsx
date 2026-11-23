import { dayText, exemplarMoveText, fenMove } from "@ci/ui/profile";
import { GamePosition } from "../../../../../components/game-position";
import { exampleLine } from "../../../../../lib/explain";
import type { Example } from "../../../../../server/profile";
import { MAX_EXAMPLES } from "../../../../../server/profile/weaknesses";

export { MAX_EXAMPLES };

export const NO_EXAMPLES_TEXT =
  "No examples from your games yet: none of your analysed games in the last 6 months has a stored case of it.";

/** How bad a move was: a mate given up or allowed outranks any drop, then the win% drop. */
const severity = (e: Example) =>
  (e.mate ? 1000 : 0) +
  (Number.isFinite(e.winBefore - e.winAfter) ? Math.max(0, e.winBefore - e.winAfter) : 0);

/** The examples worst first (critic 1); a tie goes to the one that opens in the review. */
export function orderExamples(examples: Example[], reviewIds: Map<string, string>): Example[] {
  return [...examples].sort(
    (a, b) =>
      severity(b) - severity(a) ||
      Number(reviewIds.has(b.gameId)) - Number(reviewIds.has(a.gameId)),
  );
}

/**
 * Two or three of the player's own positions (`WeaknessDetail.examples`), each a
 * `PositionExplorer` (N14): the board of the position before the move, with the move played (a
 * `pen` arrow) and the engine's move (a `good` arrow), which expands to step through the game,
 * compare the engine's line and try moves. Below it, one line on what happened to the win chance
 * and when it was played (a mate given up or allowed is named first: the Lichess mate rule judges
 * it whatever the drop). The worst move comes first; on a tie, one that opens in the review.
 */
export function Examples({
  examples,
  reviewIds,
}: {
  examples: Example[];
  /** `games.id` by platform game id. */
  reviewIds: Map<string, string>;
}) {
  if (examples.length === 0) {
    return <p className="m-0 max-w-prose text-base text-ink-soft">{NO_EXAMPLES_TEXT}</p>;
  }
  const items = orderExamples(examples, reviewIds).slice(0, MAX_EXAMPLES);
  return (
    <ol className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-10 p-0 md:grid-cols-2 xl:grid-cols-3">
      {items.map((e) => {
        const id = reviewIds.get(e.gameId);
        const move = exemplarMoveText(e.fen, e.played.san);
        const played = e.playedAt.slice(0, 10);
        const line = exampleLine(e.winBefore, e.winAfter, e.mate);
        return (
          <li key={`${e.gameId}:${e.ply}`} className="flex min-w-0 flex-col gap-2">
            <GamePosition
              className="w-full md:w-full"
              gameId={id ?? e.gameId}
              loadable={id !== undefined}
              ply={e.ply}
              fenBefore={e.fen}
              played={e.played}
              best={e.best}
              orientation={fenMove(e.fen).color}
              label={move}
              mistake
              judgement={e.judgement}
              href={id ? `/app/games/${encodeURIComponent(id)}?ply=${e.ply}` : undefined}
            />
            {line ? (
              <p className="m-0 text-base text-ink [font-variant-numeric:lining-nums]">{line}</p>
            ) : null}
            <p className="m-0 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-sm text-ink-soft">
              {/^\d{4}-\d{2}-\d{2}$/.test(played) ? (
                <span>
                  Played <time dateTime={played}>{dayText(played)}</time>
                </span>
              ) : null}
              {id ? null : <span>This game is not among your imported games.</span>}
            </p>
          </li>
        );
      })}
    </ol>
  );
}
