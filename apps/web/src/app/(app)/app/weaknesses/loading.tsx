import { PageHeading } from "../../_shell/page-heading";

const WIDTHS = [56, 44, 64, 38, 50] as const;

/**
 * The first load of `/app/weaknesses`: the rows' ruled shape in pencil wells, never a spinner
 * (B18). The line above it says what is happening.
 */
export default function Loading() {
  return (
    <div data-route-loading className="flex flex-col gap-10">
      <PageHeading title="Weaknesses" />
      <div role="status" className="flex flex-col gap-4">
        <p className="font-serif text-base text-ink-soft">Reading your weaknesses</p>
        <ul aria-hidden="true" className="m-0 list-none rounded-sheet bg-sheet p-0 py-1">
          {WIDTHS.map((w) => (
            <li
              key={w}
              className="flex h-20 items-center gap-6 border-b border-rule px-(--space-panel-pad) last:border-b-0"
            >
              <span className="h-3 rounded-pill bg-well" style={{ width: `${w}%` }} />
              <span className="ml-auto h-5 w-14 shrink-0 rounded-field bg-well" />
              <span className="hidden h-2 w-40 shrink-0 rounded-pill bg-well md:block" />
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
