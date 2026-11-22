import { dayText, ExemplarBoard, exemplarMoveText } from "@ci/ui/profile";
import Link from "next/link";
import { exampleLine } from "../../../../../lib/explain";
import type { Example } from "../../../../../server/profile";
import { MAX_EXAMPLES } from "../../../../../server/profile/weaknesses";

export { MAX_EXAMPLES };

export const NO_EXAMPLES_TEXT =
  "No examples from your games yet: none of your analysed games in the last 6 months has a stored case of it.";

/**
 * Two or three of the player's own positions (`WeaknessDetail.examples`): each board shows the
 * move played (a `pen` arrow) and the engine's move (a `good` arrow), one line on what happened
 * to the win chance, when it was played and a link to that move in the review. Positions whose
 * game is among the player's imported games come first, so most examples open.
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
  const items = [...examples]
    .sort((a, b) => Number(reviewIds.has(b.gameId)) - Number(reviewIds.has(a.gameId)))
    .slice(0, MAX_EXAMPLES);
  return (
    <ol className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-10 p-0 md:grid-cols-2 xl:grid-cols-3">
      {items.map((e) => {
        const id = reviewIds.get(e.gameId);
        const move = exemplarMoveText(e.fen, e.played.san);
        const played = e.playedAt.slice(0, 10);
        const line = exampleLine(e.winBefore, e.winAfter);
        return (
          <li key={`${e.gameId}:${e.ply}`} className="@container flex min-w-0 flex-col gap-2">
            <ExemplarBoard
              // ban-lint-allow TOK2: the board (in its 8px well from 768px) fills its column in whole-pixel squares
              className="[--board-size:round(down,100cqi,8px)] md:[--board-size:round(down,100cqi_-_16px,8px)]"
              fen={e.fen}
              played={e.played}
              best={e.best}
              refutation={e.refutationPv[0] ?? null}
              // The win chance is said once, in the line below, with its change.
              caption={{ judgement: e.judgement, winBefore: null, winAfter: null }}
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
              {id ? (
                <Link
                  href={`/app/games/${encodeURIComponent(id)}?ply=${e.ply}`}
                  className="inline-flex min-h-6 items-center gap-1 rounded-field py-0.5 text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
                >
                  Open <span className="font-serif font-semibold">{move}</span> in the review
                </Link>
              ) : (
                <span>This game is not among your imported games.</span>
              )}
            </p>
          </li>
        );
      })}
    </ol>
  );
}
