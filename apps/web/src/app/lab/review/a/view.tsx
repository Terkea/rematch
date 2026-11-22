"use client";

import { ClockPair, ReviewControls, Scoresheet } from "@ci/ui/review";
import { BoardStack, GameHeading, Graph, MoveReadout } from "../parts";
import { useLabReview } from "../use-lab-review";
import type { ReviewView } from "../view";

// Variant a, "Analysis desk": the board and the scoresheet side by side. The eval bar stands left
// of the board and the graph runs under it, the width of bar and board together; the scoresheet is
// a tall column on the right, exactly as tall as the board, controls and graph beside it.

export function ReviewA({ view: game }: { view: ReviewView }) {
  const r = useLabReview(game);
  return (
    <main
      data-lab-review="a"
      data-ply={r.ply}
      className="mx-auto grid max-w-7xl gap-x-(--space-gutter) gap-y-4 px-(--space-gutter) py-4 md:py-8 lg:grid-cols-[auto_minmax(0,1fr)]"
    >
      <GameHeading game={game} compact className="lg:hidden" />
      <div className="flex min-w-0 flex-col gap-3">
        <BoardStack r={r} />
        <ReviewControls {...r.controls} className="md:pl-7" />
        <Graph r={r} className="md:pl-7" />
      </div>
      <aside
        aria-label="Game record"
        className="flex min-h-0 min-w-0 flex-col gap-4 rounded-sheet bg-sheet p-(--space-panel-pad)"
      >
        <GameHeading game={game} className="max-lg:hidden" />
        <ClockPair {...r.clocks} tenths={r.tenths} />
        <MoveReadout r={r} className="text-xl" />
        <Scoresheet
          moves={r.sheet}
          currentPly={r.ply}
          onSelectPly={r.go}
          label={`Moves, ${r.title}`}
          announce={!r.playing}
          className="h-80 lg:h-auto lg:min-h-0 lg:flex-1 lg:basis-0"
        />
      </aside>
    </main>
  );
}
