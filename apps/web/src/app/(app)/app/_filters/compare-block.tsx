import { cn } from "@ci/ui/lib";
import { ExplainPopover } from "@ci/ui/profile";
import type { ReactNode } from "react";
import { SectionHead } from "../_overview/section-head";
import type { CompareCell, CompareView } from "./compare-view";
import { type GlossaryEntry, STATS_GLOSSARY } from "./glossary";

// The period comparison (T3): A | B | change for accuracy, blunders per game and score, with n
// on each side. A table from `md`; below it each stat is a short block, with every cell labelled.
// The change column only ever holds the verdict's words (ADR 0083 §6) and, when a verdict was
// reached, the difference with its 80% range.

/** Serif values: lining, tabular figures. */
const LINING = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
/** Sans text with figures: lining but proportional (DESIGN.md), so "64.2%" keeps its shape. */
const PROSE_NUMS = { fontVariantNumeric: "lining-nums" } as const;

function Cell({ side, cell }: { side: string; cell: CompareCell }) {
  return (
    <td className="block py-2 align-top md:table-cell md:py-4 md:pr-6">
      <span className="block text-sm text-ink-faint md:hidden">{side}</span>
      <span className="block font-serif text-xl font-semibold text-ink" style={LINING}>
        {cell.value}
      </span>
      <span className="block text-sm text-ink-soft" style={PROSE_NUMS}>
        {cell.detail}
      </span>
    </td>
  );
}

export function CompareBlock({
  view,
  id = "compare-heading",
  title = "Period A against period B",
  explain = STATS_GLOSSARY.compare,
  slot = "compare",
  head,
}: {
  view: CompareView;
  id?: string;
  title?: string;
  explain?: GlossaryEntry;
  slot?: string;
  /** Replaces the section head (a host that heads the block itself, labelled by `id`). */
  head?: ReactNode;
}) {
  return (
    <section aria-labelledby={id} data-slot={slot} className="flex flex-col gap-5">
      {head ?? <SectionHead id={id} title={title} explain={explain} />}
      <dl className="m-0 grid grid-cols-1 gap-x-8 gap-y-2 text-sm sm:grid-cols-2">
        {[view.a, view.b].map((p) => (
          <div key={p.label} className="flex flex-col">
            <dt className="font-medium text-ink">{p.label}</dt>
            <dd className="m-0 text-ink-soft" style={PROSE_NUMS}>
              {p.detail}
            </dd>
          </div>
        ))}
      </dl>
      <div
        data-surface="sheet"
        className="rounded-sheet bg-sheet px-(--space-panel-pad) py-2 md:py-4"
      >
        <table className="block w-full border-collapse md:table">
          <thead className="sr-only md:not-sr-only md:table-header-group">
            <tr>
              <th scope="col" className="pb-2 text-left text-sm font-medium text-ink-soft">
                Measure
              </th>
              <th scope="col" className="pb-2 text-left text-sm font-medium text-ink-soft">
                {view.a.label}
              </th>
              <th scope="col" className="pb-2 text-left text-sm font-medium text-ink-soft">
                {view.b.label}
              </th>
              <th scope="col" className="pb-2 text-left text-sm font-medium text-ink-soft">
                Change
              </th>
            </tr>
          </thead>
          <tbody className="block md:table-row-group">
            {view.rows.map((r) => (
              <tr
                key={r.key}
                data-row={r.key}
                className="block border-t border-rule py-3 first:border-t-0 md:table-row md:py-0 md:first:border-t"
              >
                <th
                  scope="row"
                  className="block pr-6 text-left align-top font-sans text-base font-medium text-ink md:table-cell md:py-4"
                >
                  <span className="inline-flex items-center gap-1.5">
                    {r.label}
                    <ExplainPopover entry={r.explain} />
                  </span>
                </th>
                <Cell side={view.a.label} cell={r.a} />
                <Cell side={view.b.label} cell={r.b} />
                <td className="block py-2 align-top md:table-cell md:py-4">
                  <span className="block text-sm text-ink-faint md:hidden">Change</span>
                  <span
                    data-verdict={r.change.verdict}
                    className={cn(
                      "block text-base text-ink",
                      r.change.verdict === "not_enough_games" ? "" : "font-medium",
                    )}
                  >
                    {r.change.text}
                  </span>
                  {r.change.detail ? (
                    <span className="block text-sm text-ink-soft" style={PROSE_NUMS}>
                      {r.change.detail}
                    </span>
                  ) : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
