"use client";

import { kpiFixtures } from "@ci/ui/fixtures";
import { KpiStat } from "@ci/ui/stats";
import { useEffect, useState } from "react";

// KPIs as a list, each beside its own trend and band, never a stat-banner row (B9). Replay rolls
// every value up from its previous snapshot and redraws the trends.

/** The snapshot before the latest, so Replay has somewhere to roll from. */
const previous = (trend: (number | null)[] | undefined, value: number) =>
  trend
    ?.slice(0, -1)
    .reverse()
    .find((v): v is number => v !== null) ?? value;

export function StatsSection() {
  const [run, setRun] = useState(0);
  const [rolled, setRolled] = useState(true);
  useEffect(() => {
    if (run === 0) return;
    // One frame on the old value, then roll to the new one.
    const id = requestAnimationFrame(() => setRolled(true));
    return () => cancelAnimationFrame(id);
  }, [run]);

  return (
    <section
      id="stats"
      data-section="stats"
      aria-labelledby="stats-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="stats-title" className="font-serif text-2xl font-semibold">
            Stats
          </h2>
          <p className="text-sm text-ink-soft">
            Each KPI shows its value, the range it is likely in, and the games behind it. On each
            scale the band is that range, the solid tick is the value, and the dashed tick with a
            caret is the next target. Sample numbers until real KPIs land.
          </p>
        </div>
        <button
          type="button"
          onClick={() => {
            setRolled(false);
            setRun((n) => n + 1);
          }}
          className="rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Replay
        </button>
      </div>
      <ul className="m-0 flex list-none flex-col gap-10 p-0">
        {kpiFixtures.items.map((kpi) => (
          <li key={kpi.label}>
            <KpiStat
              key={run}
              {...kpi}
              value={rolled ? kpi.value : previous(kpi.trend, kpi.value)}
              drawIn={run > 0}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}
