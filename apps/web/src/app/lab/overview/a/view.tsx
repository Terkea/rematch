import { JudgementColumns, ResultSplit } from "@ci/ui/charts";
import type { ReactNode } from "react";
import type { Overview } from "../../../../server/overview";
import { OverviewEmpty } from "../empty";
import { EstimateRow } from "../estimate-row";
import { TrendWindow } from "../trend-window";
import {
  accountText,
  classLabel,
  columnGames,
  emptyState,
  estimateView,
  recentCaption,
  reviewHref,
  splitRows,
  statusText,
  trendPoints,
  weeklySeries,
} from "../view";

/**
 * Variant a, "Scoresheet column": one reading column on a sheet, ruled like a scoresheet. The
 * head is a scoresheet's header fields (account, games, last import); below it each KPI is one
 * ruled row that carries its own chart, so a number and its evidence are never apart.
 */
export function OverviewA({ overview }: { overview: Overview }) {
  const empty = emptyState(overview);
  const { accuracy, judgements } = overview;
  return (
    <div
      data-layout="scoresheet"
      className="mx-auto w-full max-w-3xl px-(--space-gutter) py-8 md:py-12"
    >
      <div
        data-surface="sheet"
        className="rounded-sheet bg-sheet px-(--space-panel-pad) pt-6 pb-2 md:px-10 md:pt-10"
      >
        <h1 className="font-serif text-3xl font-semibold text-ink">Overview</h1>
        <HeaderFields overview={overview} />
        {empty ? (
          <div className="py-8">
            <OverviewEmpty state={empty} />
          </div>
        ) : (
          <div className="mt-6">
            <Row
              id="accuracy"
              title="Accuracy"
              caption="Each dot is a game. The line is your average over the last 20 games, with an 80% band."
            >
              <EstimateRow
                label="Average accuracy"
                view={estimateView(accuracy.overall, "percent", "analysed")}
                format="percent"
                trend={weeklySeries(judgements.weekly, "accuracy")}
                headingLevel={3}
              />
              <div data-chart="trend" className="mt-5">
                <TrendWindow
                  points={trendPoints(accuracy.points)}
                  rolling={accuracy.rolling}
                  label="Accuracy over time"
                  drawIn
                />
              </div>
            </Row>
            <Row
              id="blunders"
              title="Marked moves"
              caption="Each column is one of your last 40 analysed games, oldest on the left."
            >
              <EstimateRow
                label="Blunders per game"
                view={estimateView(judgements.blunders, "decimal", "analysed")}
                format="decimal"
                trend={weeklySeries(judgements.weekly, "blunders")}
                headingLevel={3}
              />
              <div data-chart="columns" className="mt-5">
                <JudgementColumns
                  games={columnGames(judgements.columns)}
                  label="Your last 40 analysed games"
                />
              </div>
            </Row>
            <Row id="mistakes">
              <EstimateRow
                label="Mistakes per game"
                view={estimateView(judgements.mistakes, "decimal", "analysed")}
                format="decimal"
                trend={weeklySeries(judgements.weekly, "mistakes")}
                headingLevel={3}
              />
            </Row>
            <Row id="inaccuracies">
              <EstimateRow
                label="Inaccuracies per game"
                view={estimateView(judgements.inaccuracies, "decimal", "analysed")}
                format="decimal"
                trend={weeklySeries(judgements.weekly, "inaccuracies")}
                headingLevel={3}
              />
            </Row>
            <Row
              id="time-control"
              title="By time control"
              caption="Score counts a draw as half a win."
            >
              <div data-chart="results">
                <ResultSplit rows={splitRows(overview.byTimeClass)} />
              </div>
              <div className="mt-6 flex flex-col gap-6">
                {overview.byTimeClass.map((tc) => (
                  <EstimateRow
                    key={tc.timeClass}
                    label={`${classLabel(tc.timeClass)} score`}
                    view={estimateView(tc.score, "rate", tc.timeClass)}
                    format="percent"
                    headingLevel={3}
                  />
                ))}
              </div>
            </Row>
            {overview.recent.length > 0 && (
              <Row id="recent" title="Recent games">
                <ul>
                  {overview.recent.map((g) => (
                    <li key={g.id} className="border-t border-rule first:border-t-0">
                      <a
                        href={reviewHref(g.id)}
                        className="flex min-h-11 items-center py-2 text-base text-ink underline-offset-4 hover:underline"
                      >
                        {recentCaption(g)}
                      </a>
                    </li>
                  ))}
                </ul>
              </Row>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** A scoresheet's header: labelled fields over a ruled line, filled in from the account. */
function HeaderFields({ overview }: { overview: Overview }) {
  const fields: { label: string; value: string }[] = [];
  for (const a of overview.accounts) {
    const t = accountText(a);
    fields.push({
      label: "Account",
      value: [t.name, ...t.notes.filter((n) => n === "Unverified")].join(", "),
    });
    const last = t.notes.find((n) => n.startsWith("Last imported"));
    if (last) fields.push({ label: "Last import", value: last.replace("Last imported ", "") });
  }
  fields.push({ label: "Games", value: statusText(overview.status).join(" ") });
  return (
    <dl className="mt-6 grid grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2">
      {fields.map((f) => (
        <div
          key={`${f.label}:${f.value}`}
          className="flex flex-col border-b border-rule-strong pb-1 data-[wide=true]:sm:col-span-2"
          data-wide={f.label === "Games"}
        >
          <dt className="text-sm text-ink-faint">{f.label}</dt>
          <dd className="font-serif text-lg text-ink">{f.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Row({
  id,
  title,
  caption,
  children,
}: {
  id: string;
  title?: string;
  caption?: string;
  children: ReactNode;
}) {
  return (
    <section data-kpi-row={id} className="border-t border-rule py-7 first:border-t-0">
      {title && <h2 className="font-serif text-2xl font-semibold text-ink">{title}</h2>}
      {caption && <p className="mt-1 mb-5 max-w-prose text-sm text-ink-soft">{caption}</p>}
      {children}
    </section>
  );
}
