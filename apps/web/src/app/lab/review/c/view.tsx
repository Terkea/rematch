"use client";

import { ClockPair, ReviewControls, Scoresheet } from "@ci/ui/review";
import { BoardStack, GameHeading, Graph, MomentList, MoveReadout } from "../parts";
import { useLabReview } from "../use-lab-review";
import type { ReviewView } from "../view";

// Variant c, "Annotated sheet": the scoresheet leads and reads like a coach's sheet. The shown
// move is set large with its commentary line ("31. Rxe6?? is a blunder. Win chance 64% to 22%. The
// engine preferred Bxe6."), the player's mistakes run down the margin beside the ruled moves, and
// the graph closes the sheet. The board floats beside it and stays in view while the sheet scrolls.

export function ReviewC({ view: game }: { view: ReviewView }) {
  const r = useLabReview(game);
  return (
    <main
      data-lab-review="c"
      data-ply={r.ply}
      className="mx-auto grid max-w-(--breakpoint-2xl) items-start gap-x-(--space-gutter) gap-y-4 px-(--space-gutter) py-4 md:py-8 lg:grid-cols-[minmax(0,1fr)_auto]"
    >
      <GameHeading game={game} compact className="lg:hidden" />
      <div className="flex min-w-0 flex-col gap-3 lg:sticky lg:top-6 lg:order-2">
        <BoardStack r={r} />
        <ReviewControls {...r.controls} className="md:pl-7" />
        <ClockPair {...r.clocks} tenths={r.tenths} className="md:pl-7" />
      </div>
      <article
        aria-label="Annotated scoresheet"
        className="flex min-w-0 flex-col gap-6 rounded-sheet bg-sheet p-(--space-panel-pad) lg:order-1 lg:p-8"
      >
        <GameHeading game={game} className="max-lg:hidden" />
        <section aria-label="Coach's note" aria-live="off" className="flex flex-col gap-2">
          <MoveReadout r={r} className="text-3xl" />
          <p
            data-commentary
            className="min-h-15 max-w-prose font-serif-italic text-prose text-ink italic"
          >
            {r.commentary ?? ""}
          </p>
        </section>
        <div className="grid gap-6 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
          <Scoresheet
            moves={r.sheet}
            currentPly={r.ply}
            onSelectPly={r.go}
            label={`Moves, ${r.title}`}
            announce={!r.playing}
            className="h-96 bg-paper"
          />
          <div className="flex flex-col gap-2">
            <h2 className="font-serif text-xl font-semibold text-ink">Where it went wrong</h2>
            <MomentList r={r} className="-mx-3 max-h-96 overflow-y-auto px-3" />
          </div>
        </div>
        <Graph r={r} />
      </article>
    </main>
  );
}
