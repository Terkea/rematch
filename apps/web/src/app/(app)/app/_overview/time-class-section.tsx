import { ResultSplit } from "@ci/ui/charts";
import { EstimateKpi } from "../../../../components/estimate-kpi";
import type { TimeClassView } from "./view";

const gamesText = (n: number) => (n === 1 ? "1 game" : `${n} games`);

/**
 * Each time control's results as one bar, then its score and accuracy side by side. Blitz and
 * rapid are different games, so nothing here blends them. Each class states its basis once, and
 * below the minimum states the shortfall once; its rows then show a dash.
 */
export function TimeClassSection({ section }: { section: TimeClassView }) {
  return (
    <div className="flex flex-col gap-8">
      <div data-chart="results">
        <ResultSplit rows={section.results} />
      </div>
      <div className="flex flex-col divide-y divide-rule">
        {section.classes.map((c) => {
          const cls = c.name.toLowerCase();
          const short = c.score.kind === "insufficient" ? c.score.needed : null;
          return (
            <div key={c.key} data-class={c.key} className="flex flex-col gap-4 py-5 first:pt-0">
              <p className="text-sm text-ink-soft">
                Based on {gamesText(c.games)} in {cls}.
                {short !== null &&
                  ` Need ${short} more ${short === 1 ? "game" : "games"} before a score means anything.`}
              </p>
              <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
                <EstimateKpi
                  label={`${c.name} score`}
                  estimate={c.score}
                  format="percent"
                  basisClass={cls}
                  sectionGames={c.games}
                />
                <EstimateKpi
                  label={`${c.name} accuracy`}
                  estimate={c.accuracy}
                  format="percent"
                  basisClass={cls}
                  sectionGames={c.games}
                />
              </div>
            </div>
          );
        })}
      </div>
      <p className="text-sm text-ink-soft">Score counts a draw as half a win.</p>
    </div>
  );
}
