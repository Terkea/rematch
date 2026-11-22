"use client";

import { kpiFixtures } from "@ci/ui/fixtures";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { ProgressPlaceholder } from "@ci/ui/progress";
import { KpiStat } from "@ci/ui/stats";
import { useEffect, useState } from "react";

// Placeholders stand in for the section they replace, in its shape, on a ruled sheet. Never a
// spinner (B18). The demo counts up, then the real KPI swaps in over the reveal motion.

const TOTAL = 200;

export function ProgressSection() {
  const reduced = useReducedMotion();
  const [run, setRun] = useState(0);
  const [done, setDone] = useState(38);
  const finished = done >= TOTAL;

  useEffect(() => {
    if (finished) return;
    const id = setInterval(() => setDone((d) => Math.min(TOTAL, d + 18)), 700);
    return () => clearInterval(id);
  }, [finished]);

  const kpi = kpiFixtures.items[0];

  return (
    <section
      id="progress"
      data-section="progress"
      aria-labelledby="progress-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="progress-title" className="font-serif text-2xl font-semibold">
            Progress
          </h2>
          <p className="text-sm text-ink-soft">
            While a section is still being worked out, it shows its own outline on a ruled sheet. A
            pencil stroke grows with the count when the total is known and idles when it is not.
          </p>
        </div>
        <button
          type="button"
          onClick={() => {
            setDone(38);
            setRun((n) => n + 1);
          }}
          className="rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Replay
        </button>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        <ProgressPlaceholder
          shape="graph"
          verb="Analysing"
          progress={{ done: 76, total: TOTAL, unit: "rapid games" }}
        />
        <ProgressPlaceholder
          shape="stat"
          verb="Analysing"
          basisNote="Counts update as games finish."
        />
        <ProgressPlaceholder
          shape="list"
          verb="Importing"
          progress={{ done: 120, unit: "games" }}
        />
        <ProgressPlaceholder
          shape="board"
          verb="Analysing"
          progress={{ done: 0, total: TOTAL, unit: "games" }}
        />
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="font-serif text-lg font-semibold">Swapping in</h3>
        {finished && kpi ? (
          <motion.div
            key={run}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: reduced ? dur.quick : dur.reveal, ease: ease.pen }}
          >
            <KpiStat {...kpi} drawIn={run > 0} />
          </motion.div>
        ) : (
          <ProgressPlaceholder
            shape="stat"
            verb="Analysing"
            progress={{ done, total: TOTAL, unit: "rapid games" }}
          />
        )}
      </div>
    </section>
  );
}
