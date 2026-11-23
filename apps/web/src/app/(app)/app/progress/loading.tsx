import { PageHeading } from "../../_shell/page-heading";

const BLOCKS = [
  { id: "before-after", h: "h-24" },
  { id: "rating", h: "h-40" },
  { id: "kpis", h: "h-64" },
] as const;

/** The first load of `/app/progress`: the page's ruled shape in static wells, never a spinner (B18). */
export default function Loading() {
  return (
    <div data-route-loading className="flex flex-col gap-12">
      <PageHeading title="Progress" />
      <div role="status" className="flex flex-col gap-10">
        <p className="m-0 font-serif text-base text-ink-soft">Reading your progress</p>
        {BLOCKS.map((b) => (
          <div key={b.id} aria-hidden="true" className="flex flex-col gap-4">
            <span className="block h-0.5 w-full bg-rule-strong" />
            <span className="h-4 w-48 rounded-pill bg-well" />
            <div className={`${b.h} rounded-sheet bg-sheet`} />
          </div>
        ))}
      </div>
    </div>
  );
}
