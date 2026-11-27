import { RatingTrajectory } from "@ci/ui/charts";
import { CompareBlock } from "../_filters/compare-block";
import { SectionHead } from "../_overview/section-head";
import { TRAJECTORY_GLOSSARY } from "./glossary";
import type { TrajectoryView } from "./view";

// Progress's first section (F6): the rating trajectory's headline, the rating line with its peak
// and the two compared windows, and the then | now table (the period comparison's block). Below
// the minimum it says how many more rated games it needs, with no chart.

const sheet = "rounded-sheet bg-sheet px-(--space-panel-pad) py-6 md:py-8";
const LINING = { fontVariantNumeric: "lining-nums" } as const;

export function TrajectorySection({ view }: { view: TrajectoryView }) {
  return (
    <section
      aria-labelledby="trajectory-heading"
      data-slot="trajectory"
      className="flex flex-col gap-5"
    >
      <SectionHead
        id="trajectory-heading"
        title="Rating trajectory"
        explain={view.state === "ready" ? view.explain : TRAJECTORY_GLOSSARY.trajectory}
      />
      {view.state === "not_enough_games" ? (
        <div data-surface="sheet" className={sheet}>
          <p className="m-0 max-w-2xl text-base text-ink-soft" style={LINING}>
            {view.text}
          </p>
        </div>
      ) : (
        <>
          <div data-surface="sheet" className={`${sheet} flex flex-col gap-5`}>
            <div className="flex max-w-3xl flex-col gap-2">
              <p
                data-slot="trajectory-headline"
                className="m-0 font-serif text-xl font-semibold text-ink"
                style={LINING}
              >
                {view.headline}
              </p>
              <p className="m-0 text-base text-ink-soft" style={LINING}>
                {view.facts}
              </p>
              {view.since ? (
                <p className="m-0 text-base text-ink-soft" style={LINING}>
                  {view.since}
                </p>
              ) : null}
            </div>
            <RatingTrajectory
              label={`Your ${view.timeClass} rating over time`}
              points={view.chart.points}
              peak={view.chart.peak}
              current={view.chart.current}
              windows={[view.chart.earlier, view.chart.recent]}
            />
            <p className="m-0 max-w-prose text-sm text-ink-faint" style={LINING}>
              {view.basis} The line is your rating before each game; the shaded spans are the games
              compared below.
            </p>
          </div>
          <CompareBlock
            view={view.compare}
            id="then-now-heading"
            title="Then and now"
            explain={view.explain}
            slot="then-now"
            head={
              <h3 id="then-now-heading" className="m-0 font-serif text-xl font-semibold text-ink">
                Then and now
              </h3>
            }
          />
        </>
      )}
    </section>
  );
}
