"use client";

import { JudgementColumns, ResultSplit } from "@ci/ui/charts";
import { bandText } from "@ci/ui/stats";
import { OverviewEmpty } from "../empty";
import { KpiRow } from "../estimate-row";
import { TrendWindow } from "../trend-window";
import type { EstimateView, HeaderView, ViewB } from "../view";

/**
 * Variant b, "Chart-led": the accuracy trend is the opening statement, full width on a sheet
 * band, under one sentence that states the headline numbers with their n. Below, from lg, the KPI
 * rows sit beside the judgement columns and the results by time control.
 * A client component, so it gets only `toViewB`'s narrow view model, never the `Overview`.
 */
export function OverviewB({ view }: { view: ViewB }) {
  return (
    <div data-layout="chart-led" className="flex flex-col pb-16">
      <header className="mx-auto flex w-full max-w-6xl flex-col gap-3 px-(--space-gutter) pt-8 md:pt-12">
        <h1 className="font-serif text-3xl font-semibold text-ink">Overview</h1>
        <HeaderLine header={view.header} />
        {view.empty === null && <Lead {...view.lead} />}
      </header>

      {view.empty !== null ? (
        <div className="mx-auto mt-8 w-full max-w-6xl px-(--space-gutter)">
          <OverviewEmpty state={view.empty} />
        </div>
      ) : (
        <>
          <section
            aria-labelledby="trend-heading"
            data-surface="sheet"
            className="mt-8 bg-sheet py-6 md:py-8"
          >
            <div className="px-(--space-gutter) md:px-10">
              <h2 id="trend-heading" className="sr-only">
                Accuracy over time
              </h2>
              <div data-chart="trend">
                <TrendWindow
                  points={view.trend.points}
                  rolling={view.trend.rolling}
                  label="Accuracy over time"
                  drawIn
                />
              </div>
              <p className="mt-3 text-sm text-ink-soft">
                Each dot is a game. The line is your average over the last 20 games, with an 80%
                band.
              </p>
            </div>
          </section>

          <div className="mx-auto mt-10 grid w-full max-w-6xl grid-cols-1 gap-x-12 gap-y-12 px-(--space-gutter) lg:grid-cols-12">
            <section data-region="kpis" aria-labelledby="kpi-heading" className="lg:col-span-7">
              <h2 id="kpi-heading" className="font-serif text-2xl font-semibold text-ink">
                Per game
              </h2>
              <div className="mt-6 flex flex-col gap-8">
                {view.kpis.map((row) => (
                  <div key={row.key} data-kpi-row={row.key}>
                    <KpiRow row={row} />
                  </div>
                ))}
              </div>
              <p className="mt-6 text-sm text-ink-faint">
                Sparklines show the 12 weeks up to your latest analysed game.
              </p>
            </section>

            <section
              data-region="judgements"
              aria-labelledby="judgement-heading"
              className="flex flex-col gap-10 lg:col-span-5"
            >
              <div>
                <h2 id="judgement-heading" className="font-serif text-2xl font-semibold text-ink">
                  Your last 40 games
                </h2>
                <p className="mt-1 mb-5 text-sm text-ink-soft">
                  One column per game, oldest on the left.
                </p>
                <div data-chart="columns">
                  <JudgementColumns games={view.columns} label="Your last 40 analysed games" />
                </div>
              </div>
              <div>
                <h2 className="font-serif text-2xl font-semibold text-ink">By time control</h2>
                <div data-chart="results" className="mt-5">
                  <ResultSplit rows={view.results} />
                </div>
                <div className="mt-6 flex flex-col gap-6">
                  {view.scores.map((row) => (
                    <KpiRow key={row.key} row={row} />
                  ))}
                </div>
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  );
}

function HeaderLine({ header }: { header: HeaderView }) {
  return (
    <p className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-ink-soft">
      {header.accounts.map((a) => (
        <span key={a.name}>
          <span className="font-medium text-ink">{a.name}</span>
          {a.notes.map((n) => (
            <span key={n}>, {n}</span>
          ))}
        </span>
      ))}
      <span>{header.status.join(" ")}</span>
    </p>
  );
}

/** A band sentence for the lead: the 80% range, or why there is none. */
function bandLine(name: string, v: EstimateView, format: "percent" | "decimal"): string | null {
  if (v.kind === "kpi") return `${name} ${bandText(v.interval, format)}.`;
  if (v.kind === "flat") return `${name}: ${v.note}.`;
  return null;
}

/** The headline numbers as one sentence, with their n and bands; nothing below the minimum. */
function Lead({ accuracy, blunders }: ViewB["lead"]) {
  if (accuracy.kind === "insufficient") {
    return (
      <p className="mt-4 max-w-3xl font-serif text-2xl text-ink">
        {accuracy.need} before an average accuracy means anything.
      </p>
    );
  }
  const accText = `${(accuracy.value * 100).toFixed(1)}%`;
  const blText = blunders.kind === "insufficient" ? null : blunders.value.toFixed(2);
  const bands = [
    bandLine("Accuracy", accuracy, "percent"),
    bandLine("Blunders", blunders, "decimal"),
  ]
    .filter((b): b is string => b !== null)
    .join(" ");
  return (
    <div className="mt-4 flex max-w-3xl flex-col gap-2">
      <p className="font-serif text-3xl leading-tight text-ink">
        Across {accuracy.basis.games} analysed games your accuracy averages {accText}
        {blText !== null ? `, with ${blText} blunders a game.` : "."}
      </p>
      <p className="text-sm text-ink-soft">{bands}</p>
    </div>
  );
}
