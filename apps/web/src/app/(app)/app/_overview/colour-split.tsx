import { ExplainPopover } from "@ci/ui/profile";
import { STATS_GLOSSARY } from "../_filters/glossary";
import type { ColourSplitView, SplitCell } from "./colour-split-view";

// The quiet White | Black row under the headline (T3): one column per colour, each stating its
// games, accuracy with its 80% range, blunders per game and score with W-D-L. Body weight, no
// headline numbers: the headline above stays the page's one big statement.

const LINING = { fontVariantNumeric: "lining-nums tabular-nums" } as const;

function Row({
  label,
  cell,
  explain,
}: {
  label: string;
  cell: SplitCell;
  explain: keyof typeof STATS_GLOSSARY;
}) {
  return (
    <div className="flex flex-col">
      <dt className="flex items-center gap-1 text-sm text-ink-soft">
        {label}
        <ExplainPopover entry={STATS_GLOSSARY[explain]} />
      </dt>
      <dd className="m-0 flex flex-col">
        <span className="font-serif text-lg font-semibold text-ink" style={LINING}>
          {cell.value}
        </span>
        <span className="text-sm text-ink-faint" style={LINING}>
          {cell.detail}
        </span>
      </dd>
    </div>
  );
}

export function ColourSplit({ view }: { view: ColourSplitView }) {
  return (
    <section
      aria-labelledby="colour-split-heading"
      data-slot="colour-split"
      className="flex flex-col gap-3"
    >
      <div className="flex items-center gap-1.5">
        <h2 id="colour-split-heading" className="m-0 font-sans text-base font-medium text-ink">
          By colour
        </h2>
        <ExplainPopover entry={STATS_GLOSSARY.colour_split} />
      </div>
      <div className="grid grid-cols-2 gap-x-6 gap-y-6 md:gap-x-12">
        {view.columns.map((c) => (
          <div key={c.key} data-colour={c.key} className="flex min-w-0 flex-col gap-3">
            <h3 className="m-0 flex items-baseline gap-2 font-sans text-base font-semibold text-ink">
              {c.label}
              <span className="text-sm font-normal text-ink-soft" style={LINING}>
                {c.games}
              </span>
            </h3>
            <dl className="m-0 grid grid-cols-1 gap-3 lg:grid-cols-3">
              <Row label="Accuracy" cell={c.accuracy} explain="accuracy" />
              <Row label="Blunders per game" cell={c.blunders} explain="blunders" />
              <Row label="Score" cell={c.score} explain="score" />
            </dl>
          </div>
        ))}
      </div>
    </section>
  );
}
