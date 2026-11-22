"use client";

import { Icon, ListIcon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { Drawer } from "@ci/ui/overlays";
import { buttonVariants } from "@ci/ui/primitives";
import { ClockPair, ReviewControls, Scoresheet } from "@ci/ui/review";
import { moveText } from "../model";
import { BoardStack, GameHeading, Graph, MoveReadout, ResultLines, useWide } from "../parts";
import { type LabReview, useLabReview } from "../use-lab-review";
import type { ReviewView } from "../view";

// Variant b, "Graph as the spine": the win-chance graph runs the full width on top and drives the
// board under it; the player's mistakes hang off the graph as jumps. Beside the board sit the shown
// move, its coach's line, the clocks and (from 768px) the scoresheet. On phones the scoresheet is
// a drawer, so the graph, board, eval bar and controls share the first screen.

function MomentStrip({ r, className }: { r: LabReview; className?: string }) {
  if (r.moments.length === 0) return null;
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <h2 className="text-sm text-ink-soft">Your mistakes, in game order</h2>
      <ol className="flex flex-wrap gap-2">
        {r.moments.map((ply) => (
          <li key={ply}>
            <button
              type="button"
              onClick={() => r.go(ply)}
              aria-current={ply === r.ply ? "step" : undefined}
              className={cn(
                "inline-flex min-h-11 items-center rounded-field border border-rule px-3 font-serif font-semibold",
                "text-pen-text [font-variant-numeric:lining-nums_tabular-nums] hover:bg-well",
                "aria-[current=step]:border-ink aria-[current=step]:bg-ink aria-[current=step]:text-on-ink",
                "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
              )}
            >
              {moveText(r.game, ply)}
            </button>
          </li>
        ))}
      </ol>
    </div>
  );
}

function Sheet({ r, className }: { r: LabReview; className?: string }) {
  return (
    <Scoresheet
      moves={r.sheet}
      currentPly={r.ply}
      onSelectPly={r.go}
      label={`Moves, ${r.title}`}
      announce={!r.playing}
      className={className}
    />
  );
}

export function ReviewB({ view: game }: { view: ReviewView }) {
  const r = useLabReview(game);
  const wide = useWide();
  return (
    <main
      data-lab-review="b"
      data-ply={r.ply}
      className="mx-auto flex max-w-7xl flex-col gap-4 px-(--space-gutter) py-4 md:py-6"
    >
      <GameHeading game={game} compact brief />
      <section
        aria-label="Win chance"
        className="rounded-sheet bg-sheet p-(--space-panel-pad) max-md:-mx-(--space-gutter) max-md:rounded-none max-md:py-2"
      >
        <Graph r={r} />
      </section>
      <div className="grid gap-x-(--space-gutter) gap-y-4 lg:grid-cols-[auto_minmax(0,1fr)]">
        <div className="flex min-w-0 flex-col gap-3">
          <BoardStack r={r} />
          <ReviewControls {...r.controls} className="md:pl-7" />
        </div>
        <div className="flex min-w-0 flex-col gap-4">
          <div className="flex flex-col gap-2">
            <MoveReadout r={r} className="text-3xl" />
            <p className="min-h-15 max-w-prose font-serif text-prose text-ink">
              {r.commentary ?? ""}
            </p>
          </div>
          <ClockPair {...r.clocks} tenths={r.tenths} />
          <MomentStrip r={r} />
          <ResultLines game={game} />
          {/* The drawer stays mounted (its trigger hides from 768px), so hydration never unmounts it. */}
          <Drawer.Root>
            <Drawer.Trigger
              className={cn(buttonVariants({ variant: "outline" }), "self-start md:hidden")}
            >
              <Icon icon={ListIcon} size={20} />
              Moves
            </Drawer.Trigger>
            <Drawer.Content aria-describedby={undefined}>
              <Drawer.Title>Moves</Drawer.Title>
              <Sheet r={r} className="h-96" />
            </Drawer.Content>
          </Drawer.Root>
          {/* One sheet in the DOM at a time: the inline one only from 768px (the drawer's is closed). */}
          {wide ? (
            <Sheet r={r} className="h-96 bg-sheet lg:h-auto lg:min-h-72 lg:flex-1 lg:basis-0" />
          ) : null}
        </div>
      </div>
    </main>
  );
}
