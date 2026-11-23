import { JudgementColumns } from "@ci/ui/charts";
import { EstimateKpi } from "../../../../components/estimate-kpi";
import { type GlossaryEntry, KPI_GLOSSARY } from "../../../../server/profile/glossary";
import { STATS_GLOSSARY } from "../_filters/glossary";
import type { JudgementsView } from "./view";

/** Each row's "What is this?": the KPI glossary's blunders, the game stats' other two (A10). */
const EXPLAIN: Record<string, GlossaryEntry | undefined> = {
  blunders: KPI_GLOSSARY.blunders_per_game,
  mistakes: STATS_GLOSSARY.mistakes,
  inaccuracies: STATS_GLOSSARY.inaccuracies,
};

const scopeText = (j: JudgementsView) =>
  `Across ${j.games} ${j.basisClass} ${j.games === 1 ? "game" : "games"}.`;

/**
 * Blunders, mistakes and inaccuracies per game, each with its weekly sparkline (first and last
 * values, and "Lower is better"), then one column per recent game. n is stated once, here; a row
 * on another basis states its own.
 */
export function JudgementsSection({ judgements: j }: { judgements: JudgementsView }) {
  const n = j.columns.length;
  const first = j.rows[0]?.estimate;
  const short = first?.kind === "insufficient" ? first.needed : null;
  const columnsLabel = `Your last ${n} analysed ${n === 1 ? "game" : "games"}`;
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-1 text-sm text-ink-soft">
        <p>{scopeText(j)}</p>
        {short !== null && (
          <p>
            Need {short} more {short === 1 ? "game" : "games"} before these rates mean anything.
          </p>
        )}
        {j.rows.some((r) => r.estimate.kind === "estimate") && (
          <p>
            Each small line is your latest 12 weeks of play, one point a week. A gap is a week with
            no games.
          </p>
        )}
      </div>
      <div className="flex flex-col divide-y divide-rule">
        {j.rows.map((row) => (
          <div key={row.key} data-kpi-row={row.key} className="py-5 first:pt-0">
            <EstimateKpi
              label={row.label}
              estimate={row.estimate}
              format="decimal"
              basisClass={j.basisClass}
              trend={row.trend}
              trendGames={row.trendGames}
              sectionGames={j.games}
              better="lower"
              explain={EXPLAIN[row.key] ?? null}
            />
          </div>
        ))}
      </div>
      {n > 0 && (
        <div className="flex flex-col gap-3">
          <div>
            <h3 className="text-base font-medium text-ink">{columnsLabel}</h3>
            <p className="text-sm text-ink-soft">One column per game, newest on the right.</p>
          </div>
          <div data-chart="columns">
            <JudgementColumns games={j.columns} label={columnsLabel} />
          </div>
        </div>
      )}
    </div>
  );
}
