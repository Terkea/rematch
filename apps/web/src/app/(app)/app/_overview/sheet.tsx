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
 * The overview, variant b (ADR 0053): a headline sentence, the red-pen slot, the accuracy trend
 * on a sheet, then mistakes per game, time controls and recent games as ruled sections. With
 * `reveal` (the first view in a session) the first viewport assembles once: rules draw, the
 * trend draws in, numbers roll. Nothing below the fold animates.
 */
export function OverviewSheet({
  view,
  reveal,
  sync,
  lead = { kind: "building" },
  filterBar,
  colourSplit,
  compare,
  filtersNote,
  peer = null,
  trajectory,
  focus,
}: {
  view: OverviewView;
  reveal: boolean;
  /** The compact sync and analysis panel (Task P2); it replaces the status line when given. */
  sync?: ReactNode;
  /** The coaching profile's lead finding for "What to work on" (default: still building). */
  lead?: LeadFinding;
  /** The shared filter bar (T3); without it, the plain time-control switch. */
  filterBar?: ReactNode;
  /** The White | Black row under the headline, from `gameStats` with the filters applied. */
  colourSplit?: ColourSplitView | null;
  /** Period A against period B, when the URL asks for a comparison. */
  compare?: CompareView | null;
  /** What the filters beyond the class apply to, when any is set. */
  filtersNote?: string | null;
  /** The players faced, for the headline's comparison (Q1); null without a peer benchmark. */
  peer?: HeadlinePeer | null;
  /** The rating trajectory's headline card (F6), under the headline. */
  trajectory?: ReactNode;
  /** The weekly focus card (ADR 0091), first on the page. */
  focus?: ReactNode;
}) {
  return (
    <div {...(reveal ? { "data-reveal": "first" } : {})} className="flex flex-col gap-10">
      {reveal && <RevealSeen />}
      <header className="flex flex-col gap-4">
        <h1 className="font-serif text-3xl font-semibold text-ink">Overview</h1>
        {sync ?? <StatusLine header={view.header} />}
      </header>

      {view.empty !== null ? (
        <OverviewEmpty state={view.empty} />
      ) : (
        <>
          {focus}
          <div className="flex flex-col gap-6">
            {filterBar ?? <TimeClassSwitch options={view.switch} />}
            {/* An asked-for comparison answers the question just set, so it comes first
                (critic 4); on a phone it was some 1,100 px down. */}
            {compare ? <CompareBlock view={compare} /> : null}
            <div data-reveal-fig="" style={{ "--reveal-i": 0 } as React.CSSProperties}>
              <Headline lead={view.lead} basisClass={view.basisClass} reveal={reveal} peer={peer} />
            </div>
            {trajectory}
            {filtersNote ? (
              <p data-slot="filters-note" className="m-0 max-w-prose text-sm text-ink-soft">
                {filtersNote}
              </p>
            ) : null}
            {colourSplit && colourSplit.columns.length > 0 ? (
              <ColourSplit view={colourSplit} />
            ) : null}
            <WorkOn lead={lead} />
          </div>

          <section aria-labelledby="accuracy-heading" className="flex flex-col gap-5">
            <SectionHead
              id="accuracy-heading"
              title="Accuracy over time"
              order={1}
              explain={KPI_GLOSSARY.accuracy}
            />
            <div
              data-surface="sheet"
              className="rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:py-8"
            >
              <AccuracySection accuracy={view.accuracy} drawIn={reveal} />
            </div>
          </section>

          <section aria-labelledby="judgements-heading" className="flex flex-col gap-5">
            <SectionHead id="judgements-heading" title="Mistakes per game" />
            <JudgementsSection judgements={view.judgements} />
          </section>

          <div className="grid grid-cols-1 gap-x-12 gap-y-10 lg:grid-cols-12">
            <section aria-labelledby="tc-heading" className="flex flex-col gap-5 lg:col-span-5">
              <SectionHead id="tc-heading" title="By time control" />
              <TimeClassSection section={view.byTimeClass} />
            </section>
            <section aria-labelledby="recent-heading" className="flex flex-col gap-5 lg:col-span-7">
              <SectionHead id="recent-heading" title="Recent games">
                <p className="text-sm text-ink-soft">Open a game to replay it move by move.</p>
              </SectionHead>
              <RecentSection rows={view.recent} scope={view.scopeName} />
            </section>
          </div>
        </>
      )}
    </div>
  );
}
