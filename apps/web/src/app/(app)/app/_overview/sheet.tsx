import type { ReactNode } from "react";
import type { HeadlinePeer } from "../../../../lib/explain";
import { KPI_GLOSSARY } from "../../../../server/profile/glossary";
import type { LeadFinding } from "../../../../server/profile/types";
import { CompareBlock } from "../_filters/compare-block";
import type { CompareView } from "../_filters/compare-view";
import { AccuracySection } from "./accuracy-section";
import { ColourSplit } from "./colour-split";
import type { ColourSplitView } from "./colour-split-view";
import { OverviewEmpty } from "./empty-state";
import { Headline, WorkOn } from "./headline";
import { JudgementsSection } from "./judgements-section";
import { RecentSection } from "./recent-section";
import { RevealSeen } from "./reveal-seen";
import { SectionHead } from "./section-head";
import { StatusLine } from "./status-line";
import { TimeClassSection } from "./time-class-section";
import { TimeClassSwitch } from "./time-class-switch";
import type { OverviewView } from "./view";

/**
 * The overview (ADR 0092, after variant b of ADR 0053): a focused flow for an 800 to 1200 player.
 * Where you are (the trajectory), this week's focus, one practise call, how you're doing (the
 * headline and White | Black for the time control), then everything else under "Details",
 * quieter, with the filter bar that scopes it and the period comparison right under the bar. With
 * `reveal` (the first view in a session) the first viewport assembles once; nothing below the
 * fold animates.
 */
export function OverviewSheet({
  view,
  reveal,
  sync,
  lead = { kind: "building" },
  timeControl,
  filterBar,
  colourSplit,
  filteredSplit,
  compare,
  filtersNote,
  peer = null,
  trajectory,
  focus,
  practise,
}: {
  view: OverviewView;
  reveal: boolean;
  /** The compact sync and analysis panel (Task P2); it replaces the status line when given. */
  sync?: ReactNode;
  /** The coaching profile's lead finding for "What to work on" (default: still building). */
  lead?: LeadFinding;
  /** The page-wide time-control switch; without it, the plain one from the view. */
  timeControl?: ReactNode;
  /** The shared filter bar (T3) without its time control: it scopes the Details. */
  filterBar?: ReactNode;
  /** The White | Black row for the whole time control, under the headline. */
  colourSplit?: ColourSplitView | null;
  /** The same row with the filter bar's format, colour and dates, in Details; null when unset. */
  filteredSplit?: ColourSplitView | null;
  /** Period A against period B, when the URL asks for a comparison. */
  compare?: CompareView | null;
  /** What the filters apply to, when any is set. */
  filtersNote?: string | null;
  /** The players faced, for the headline's comparison (Q1); null without a peer benchmark. */
  peer?: HeadlinePeer | null;
  /** "Where you are": the rating trajectory's card (F6), first. */
  trajectory?: ReactNode;
  /** The weekly focus card (ADR 0091), second. */
  focus?: ReactNode;
  /** The one practise call (ADR 0092), third. */
  practise?: ReactNode;
}) {
  return (
    <div {...(reveal ? { "data-reveal": "first" } : {})} className="flex flex-col gap-10">
      {reveal && <RevealSeen />}
      <header className="flex flex-col gap-4">
        <h1 className="font-serif text-3xl font-semibold text-ink">Overview</h1>
        {sync ?? <StatusLine header={view.header} />}
        {view.empty === null ? (timeControl ?? <TimeClassSwitch options={view.switch} />) : null}
      </header>

      {view.empty !== null ? (
        <OverviewEmpty state={view.empty} />
      ) : (
        <>
          {trajectory || focus || practise ? (
            <div className="flex flex-col gap-4">
              {trajectory}
              {focus}
              {practise}
            </div>
          ) : null}

          <section aria-labelledby="how-heading" className="flex flex-col gap-5">
            <SectionHead
              id="how-heading"
              title="How you're doing"
              {...(reveal ? { order: 0 } : {})}
            />
            <div data-reveal-fig="" style={{ "--reveal-i": 1 } as React.CSSProperties}>
              <Headline lead={view.lead} basisClass={view.basisClass} reveal={reveal} peer={peer} />
            </div>
            {colourSplit && colourSplit.columns.length > 0 ? (
              <ColourSplit view={colourSplit} />
            ) : null}
          </section>

          <section aria-labelledby="details-heading" className="flex flex-col gap-8">
            <SectionHead id="details-heading" title="Details">
              <p className="m-0 max-w-prose text-sm text-ink-soft">
                The numbers behind the summary, with filters for a closer look.
              </p>
            </SectionHead>
            <div className="flex flex-col gap-6">
              {filterBar}
              {/* An asked-for comparison answers the question just set, so it comes first
                  (critic 4); on a phone it was some 1,100 px down. */}
              {compare ? <CompareBlock view={compare} /> : null}
              {filtersNote ? (
                <p data-slot="filters-note" className="m-0 max-w-prose text-sm text-ink-soft">
                  {filtersNote}
                </p>
              ) : null}
              {filteredSplit && filteredSplit.columns.length > 0 ? (
                <ColourSplit view={filteredSplit} filtered />
              ) : null}
              <WorkOn lead={lead} />
            </div>

            <section aria-labelledby="accuracy-heading" className="flex flex-col gap-5">
              <SectionHead
                id="accuracy-heading"
                title="Accuracy over time"
                level={3}
                explain={KPI_GLOSSARY.accuracy}
              />
              <div
                data-surface="sheet"
                className="rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:py-8"
              >
                <AccuracySection accuracy={view.accuracy} drawIn={false} />
              </div>
            </section>

            <section aria-labelledby="judgements-heading" className="flex flex-col gap-5">
              <SectionHead id="judgements-heading" title="Mistakes per game" level={3} />
              <JudgementsSection judgements={view.judgements} />
            </section>

            <div className="grid grid-cols-1 gap-x-12 gap-y-10 lg:grid-cols-12">
              <section aria-labelledby="tc-heading" className="flex flex-col gap-5 lg:col-span-5">
                <SectionHead id="tc-heading" title="By time control" level={3} />
                <TimeClassSection section={view.byTimeClass} />
              </section>
              <section
                aria-labelledby="recent-heading"
                className="flex flex-col gap-5 lg:col-span-7"
              >
                <SectionHead id="recent-heading" title="Recent games" level={3}>
                  <p className="text-sm text-ink-soft">Open a game to replay it move by move.</p>
                </SectionHead>
                <RecentSection rows={view.recent} scope={view.scopeName} />
              </section>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
