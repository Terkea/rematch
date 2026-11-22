"use client";

import { JudgementColumns, ResultSplit } from "@ci/ui/charts";
import { CaretDownIcon, Icon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { bandText, Stat } from "@ci/ui/stats";
import { useId, useState } from "react";
import type { TimeClass } from "../../../../server/games/types";
import { OverviewEmpty } from "../empty";
import { KpiRow } from "../estimate-row";
import { TrendWindow } from "../trend-window";
import { basisLine, type SpineRowView, type ViewC } from "../view";

/**
 * Variant c, "By time control first": the time controls are the spine. Each row states its
 * games, its results and its score with n; opening a row shows that time control's own accuracy
 * trend and judgements. The time control with the most analysed games opens first.
 * A client component, so it gets only `toViewC`'s narrow view model, never the `Overview`.
 */
export function OverviewC({ view }: { view: ViewC }) {
  const [open, setOpen] = useState<Set<TimeClass>>(() => new Set(view.open ? [view.open] : []));
  const toggle = (tc: TimeClass) =>
    setOpen((s) => {
      const next = new Set(s);
      if (next.has(tc)) next.delete(tc);
      else next.add(tc);
      return next;
    });
  const { accuracy, blunders } = view.summary;

  return (
    <div
      data-layout="time-control"
      className="mx-auto w-full max-w-5xl px-(--space-gutter) py-8 md:py-12"
    >
      <header className="flex flex-col gap-2">
        <h1 className="font-serif text-3xl font-semibold text-ink">Overview</h1>
        <p className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-ink-soft">
          {view.header.accounts.map((a) => (
            <span key={a.name}>
              <span className="font-medium text-ink">{a.name}</span>
              {a.notes.map((n) => (
                <span key={n}>, {n}</span>
              ))}
            </span>
          ))}
          <span>{view.header.status.join(" ")}</span>
        </p>
        {view.empty === null && (
          <p className="text-base text-ink-soft">
            All time controls:{" "}
            {accuracy.kind === "insufficient"
              ? `accuracy: ${accuracy.need.toLowerCase()}`
              : `accuracy ${(accuracy.value * 100).toFixed(1)}%`}
            {blunders.kind !== "insufficient" && `, ${blunders.value.toFixed(2)} blunders a game`}
            {accuracy.kind === "insufficient" ? "" : ` (${accuracy.basis.games} analysed games)`}.
          </p>
        )}
      </header>

      {view.empty !== null ? (
        <div className="mt-8">
          <OverviewEmpty state={view.empty} />
        </div>
      ) : (
        <ul data-spine className="mt-8 flex flex-col gap-2">
          {view.rows.map((row) => (
            <SpineRow
              key={row.timeClass}
              row={row}
              open={open.has(row.timeClass)}
              onToggle={() => toggle(row.timeClass)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function SpineRow({
  row,
  open,
  onToggle,
}: {
  row: SpineRowView;
  open: boolean;
  onToggle: () => void;
}) {
  const id = useId();
  const reduced = useReducedMotion();
  const { score } = row;
  return (
    <li
      data-time-class={row.timeClass}
      data-open={open}
      data-surface={open ? "sheet" : undefined}
      className={cn(
        "rounded-sheet transition-colors",
        open ? "bg-sheet" : "bg-paper hover:bg-sheet",
      )}
    >
      <div className="grid grid-cols-1 items-center gap-x-8 gap-y-3 px-(--space-panel-pad) py-5 md:grid-cols-12">
        <h2 className="md:col-span-3">
          <button
            type="button"
            aria-expanded={open}
            aria-controls={`${id}-panel`}
            onClick={onToggle}
            className="flex min-h-11 w-full items-center gap-3 rounded-field text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            <motion.span
              aria-hidden="true"
              className="inline-flex text-ink-soft"
              animate={{ rotate: open ? 0 : -90 }}
              transition={reduced ? { duration: 0 } : { duration: dur.quick, ease: ease.out }}
            >
              <Icon icon={CaretDownIcon} />
            </motion.span>
            <span className="flex flex-col">
              <span className="font-serif text-2xl font-semibold text-ink">{row.name}</span>
              <span className="text-sm text-ink-faint">
                {row.games === 1 ? "1 analysed game" : `${row.games} analysed games`}
              </span>
            </span>
          </button>
        </h2>
        <div className="md:col-span-5">
          <ResultSplit rows={[row.split]} />
        </div>
        <div className="md:col-span-4 md:text-right">
          <p className="text-sm text-ink-faint">Score</p>
          {score.kind === "insufficient" ? (
            <>
              <p className="font-serif text-lg text-ink-soft">{score.need}</p>
              <p className="mt-1 text-sm text-ink-faint">{score.basis}</p>
            </>
          ) : (
            <>
              <Stat
                value={score.value}
                format="percent"
                className="font-serif text-3xl leading-none font-semibold text-ink"
              />
              <p className="mt-1 text-sm text-ink-soft">
                {score.kind === "kpi" ? bandText(score.interval, "percent") : score.note}
              </p>
              <p className="text-sm text-ink-faint">
                {basisLine(score.basis.games, score.basis.timeClass)}
              </p>
            </>
          )}
        </div>
      </div>
      <div id={`${id}-panel`} hidden={!open}>
        {open && row.detail && (
          <motion.div
            initial={reduced ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: dur.quick, ease: ease.out }}
            className="grid grid-cols-1 gap-x-12 gap-y-10 border-t border-rule px-(--space-panel-pad) pt-6 pb-8 lg:grid-cols-2"
          >
            <ClassDetail name={row.name} detail={row.detail} />
          </motion.div>
        )}
      </div>
    </li>
  );
}

function ClassDetail({
  name,
  detail,
}: {
  name: string;
  detail: NonNullable<SpineRowView["detail"]>;
}) {
  return (
    <>
      <section data-kpi-row="accuracy" className="flex flex-col gap-5">
        <KpiRow row={detail.accuracy} />
        <div data-chart="trend">
          <TrendWindow
            points={detail.trend.points}
            rolling={detail.trend.rolling}
            label={`${name} accuracy over time`}
            drawIn
          />
        </div>
      </section>
      <section data-kpi-row="blunders" className="flex flex-col gap-6">
        {detail.kpis.map((row) => (
          <KpiRow key={row.key} row={row} />
        ))}
        <div data-chart="columns">
          <JudgementColumns games={detail.columns} label={detail.columnsLabel} />
        </div>
      </section>
    </>
  );
}
