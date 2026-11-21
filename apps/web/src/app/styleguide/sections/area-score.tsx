"use client";

import { AreaScoreKey, AreaScoreList } from "@ci/ui/area-score";
import { areaScoreFixtures } from "@ci/ui/fixtures";
import { useState } from "react";

// The seven areas as ruler rows, from the synthetic fixtures (spec 8b) until M2 lands.

export function AreaScoreSection() {
  const [run, setRun] = useState(0);
  return (
    <section
      id="area-score"
      data-section="area-score"
      aria-labelledby="area-score-title"
      className="flex flex-col gap-5 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="area-score-title" className="font-serif text-2xl font-semibold">
            Area score
          </h2>
          <p className="text-sm text-ink-soft">
            Each area on one shared ruler, scaled to these scores: your score against the median of
            players at your rating, and the score that would put you in the next band.
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
      <AreaScoreKey />
      <AreaScoreList key={run} drawIn={run > 0} items={areaScoreFixtures.items} />
    </section>
  );
}
