import Link from "next/link";
import type { TrajectoryView } from "./view";

// The trajectory's compact card for the Overview (F6): the headline and a link to what changed
// on Progress. Below the minimum it renders nothing, so the overview reads as before. Exported
// for the overview lane to place; it sits under the headline until then.

const LINING = { fontVariantNumeric: "lining-nums" } as const;

export function TrajectoryCard({ view, href }: { view: TrajectoryView; href: string }) {
  if (view.state !== "ready") return null;
  return (
    <section
      aria-labelledby="trajectory-card-heading"
      data-slot="trajectory-card"
      data-surface="sheet"
      className="flex flex-col gap-2 rounded-sheet bg-sheet px-(--space-panel-pad) py-5"
    >
      <h2 id="trajectory-card-heading" className="m-0 text-sm font-medium text-ink-soft">
        Rating trajectory
      </h2>
      <p className="m-0 max-w-3xl font-serif text-xl font-semibold text-ink" style={LINING}>
        {view.headline}
      </p>
      <p className="m-0 text-sm text-ink-soft" style={LINING}>
        {view.basis}{" "}
        <Link
          href={href}
          className="rounded-field font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          See what changed
        </Link>
      </p>
    </section>
  );
}
