"use client";

import { MarginNote, PencilTexture } from "@ci/ui/brand";

// The Rematch brand's hand-made touches (DESIGN.md, "Hand-made touches"): dad's margin note, the
// crayon under a celebration and the pencil grain. They live only in soft places, so the demos sit
// in the empty state and the solved drill they belong to, never beside a chart.

export function BrandSection() {
  return (
    <section
      id="brand"
      data-section="brand"
      aria-labelledby="brand-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="brand-title" className="font-serif text-2xl font-semibold">
          Hand-made touches
        </h2>
        <p className="text-sm text-ink-soft">
          The hand-made touches show only where nothing is being measured: empty states, the landing
          page, a drill solved and milestones. Boards, graphs, tables and numbers stay crisp.
        </p>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        <div className="flex flex-col gap-3">
          <h3 className="font-serif text-xl font-semibold">Empty state</h3>
          <div className="relative flex flex-col items-start gap-4 overflow-hidden rounded-sheet bg-paper px-(--space-panel-pad) py-8">
            <PencilTexture seed="styleguide-empty" />
            <p className="relative max-w-prose font-serif text-xl font-semibold text-ink">
              Connect a chess.com account to see your games here.
            </p>
            <MarginNote className="relative">let's go through your games</MarginNote>
          </div>
          <p className="text-sm text-ink-soft">
            The note repeats the sentence above it, so screen readers skip it.
          </p>
        </div>

        <div className="flex flex-col gap-3">
          <h3 className="font-serif text-xl font-semibold">A drill solved</h3>
          <div className="flex flex-col items-start gap-4 rounded-sheet bg-paper px-(--space-panel-pad) py-8">
            <p className="m-0 font-serif text-xl font-semibold text-good-text">
              Right: Rd8# mates. (Morphy, Paris 1858)
            </p>
            <MarginNote celebrate>there you go</MarginNote>
          </div>
          <p className="text-sm text-ink-soft">
            The crayon is only for a celebration. The verdict says what happened; the note and the
            crayon are decoration.
          </p>
        </div>

        <div className="flex flex-col gap-3 md:col-span-2">
          <h3 className="font-serif text-xl font-semibold">A note with its own words</h3>
          <div className="flex flex-col items-start gap-4 rounded-sheet bg-paper px-(--space-panel-pad) py-8">
            <MarginNote label="You moved the same knight again">same knight again</MarginNote>
          </div>
          <p className="text-sm text-ink-soft">
            When a note says something the page doesn't, its label is what a screen reader hears.
          </p>
        </div>
      </div>
    </section>
  );
}
