import { PageHeading } from "../../_shell/page-heading";

const SECTIONS = [
  { id: "above", rows: [52, 40] },
  { id: "improving", rows: [46] },
  { id: "openings", rows: [58, 36] },
] as const;

/**
 * The first load of `/app/strengths`: the sections' ruled heads and rows in pencil wells, then
 * the boards' squares, never a spinner (B18). The line above them says what is happening.
 */
export default function Loading() {
  return (
    <div data-route-loading className="flex flex-col gap-12">
      <PageHeading title="Strengths" />
      <div role="status" className="flex flex-col gap-10">
        <p className="font-serif text-base text-ink-soft">Reading your strengths</p>
        {SECTIONS.map((s) => (
          <div key={s.id} aria-hidden="true" className="flex flex-col gap-5">
            <span className="block h-0.5 w-full bg-rule-strong" />
            <span className="h-5 w-64 max-w-full rounded-pill bg-well" />
            <ul className="m-0 list-none rounded-sheet bg-sheet p-0 py-1">
              {s.rows.map((w) => (
                <li
                  key={w}
                  className="flex h-20 items-center gap-6 border-b border-rule px-(--space-panel-pad) last:border-b-0"
                >
                  <span className="h-3 rounded-pill bg-well" style={{ width: `${w}%` }} />
                  <span className="ml-auto hidden h-2 w-40 shrink-0 rounded-pill bg-well md:block" />
                  <span className="h-5 w-14 shrink-0 rounded-field bg-well" />
                </li>
              ))}
            </ul>
          </div>
        ))}
        <div aria-hidden="true" className="grid grid-cols-1 gap-8 md:grid-cols-2 xl:grid-cols-3">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className={`aspect-square w-full rounded-board bg-well ${i > 0 ? "max-md:hidden" : ""} ${i > 1 ? "max-xl:hidden" : ""}`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
