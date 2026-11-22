import { dayText, ExemplarBoard, exemplarMoveText } from "@ci/ui/profile";
import Link from "next/link";
import type { WeaknessDetail } from "../../../../../server/profile";

/** At most this many example positions on a weakness page. */
export const MAX_EXEMPLARS = 5;

export const NO_REPORT_TEXT = "Example positions appear once your first report is built.";

const day = (d: Date) => d.toISOString().slice(0, 10);

/**
 * The weakness's example positions from the newest report input (ruling R2): each board with
 * its arrows and caption, the game's date, and a link to that move in the review. The heading
 * says which report they come from; without a report the page says when they will appear.
 */
export function Exemplars({
  exemplars,
  reviewIds,
}: {
  exemplars: WeaknessDetail["exemplars"];
  /** `games.id` by the platform game id the report cites. */
  reviewIds: Map<string, string>;
}) {
  if (exemplars === null) {
    return <p className="text-base text-ink-soft">{NO_REPORT_TEXT}</p>;
  }
  const from = day(exemplars.fromReportAt);
  const items = exemplars.items.slice(0, MAX_EXEMPLARS);
  return (
    <div className="flex flex-col gap-6">
      <p className="text-base text-ink-soft">
        Examples from your report of <time dateTime={from}>{dayText(from)}</time>
      </p>
      {items.length === 0 ? (
        <p className="text-base text-ink-soft">
          That report has no example positions for this weakness.
        </p>
      ) : (
        <ol className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-10 p-0 md:grid-cols-2 xl:grid-cols-3">
          {items.map((e, i) => {
            const id = reviewIds.get(e.gameId);
            const move = exemplarMoveText(e.fen, e.played.san);
            const played = e.playedAt.slice(0, 10);
            return (
              // A report may cite one position twice; its place in the list keeps keys unique.
              // biome-ignore lint/suspicious/noArrayIndexKey: the list is fixed for the render
              <li key={`${e.gameId}:${e.ply}:${i}`} className="@container flex min-w-0 flex-col">
                <ExemplarBoard
                  // ban-lint-allow TOK2: the board (in its 8px well from 768px) fills its column in whole-pixel squares
                  className="[--board-size:round(down,100cqi,8px)] md:[--board-size:round(down,100cqi_-_16px,8px)]"
                  fen={e.fen}
                  played={e.played}
                  best={e.best}
                  refutation={e.refutationPv[0] ?? null}
                  caption={{ winBefore: e.winBefore, winAfter: e.winAfter }}
                />
                <p className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-sm text-ink-soft">
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
      )}
    </div>
  );
}
