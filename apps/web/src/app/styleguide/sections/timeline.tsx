"use client";

import { timelineFixtures } from "@ci/ui/fixtures";
import { WeaknessTimeline } from "@ci/ui/timeline";
import { useState } from "react";

// Weakness swimlanes from the synthetic fixtures (spec 12b) until M2 lands. "Rook endgames" runs
// through every state: it emerges, turns active, improves after the plan, resolves and relapses.

export function TimelineSection() {
  const [run, setRun] = useState(0);
  return (
    <section
      id="timeline"
      data-section="timeline"
      aria-labelledby="timeline-title"
      className="flex flex-col gap-5 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="timeline-title" className="font-serif text-2xl font-semibold">
            Weakness timeline
          </h2>
          <p className="text-sm text-ink-soft">
            When each weakness started, when it improved and whether it came back.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setRun((n) => n + 1)}
          className="rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Replay draw
        </button>
      </div>
      <WeaknessTimeline key={run} drawIn={run > 0} {...timelineFixtures} />
    </section>
  );
}
