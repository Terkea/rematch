import Link from "next/link";
import type { TrajectoryView } from "./view";

// The trajectory's card that opens the Overview (F6, ADR 0092), "Where you are": the current
// rating, the peak-against-now headline with its n, and a link to what changed on Progress.
// Below the minimum it says how many more rated games it needs and shows no number.

const LINING = { fontVariantNumeric: "lining-nums" } as const;
const LINK =
  "rounded-field font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink";
const count = new Intl.NumberFormat("en-GB");

function Card({ children }: { children: React.ReactNode }) {
  return (
    <section
      aria-labelledby="trajectory-card-heading"
      data-slot="trajectory-card"
      data-surface="sheet"
      className="flex max-w-3xl flex-col gap-3 rounded-sheet bg-sheet px-(--space-panel-pad) py-5"
    >
      <h2 id="trajectory-card-heading" className="m-0 text-base font-medium text-ink-soft">
        Where you are
      </h2>
      {children}
    </section>
  );
}

export function TrajectoryCard({ view, href }: { view: TrajectoryView; href: string }) {
  if (view.state !== "ready") {
    return (
      <Card>
        <p className="m-0 max-w-prose text-base text-ink" style={LINING}>
          {view.text}{" "}
          <Link href={href} className={LINK}>
            See your progress
          </Link>
        </p>
      </Card>
    );
  }
  const tc = view.timeClass.charAt(0).toUpperCase() + view.timeClass.slice(1);
  return (
    <Card>
      <div className="flex flex-col gap-3 md:flex-row md:items-start md:gap-8">
        <p className="m-0 flex shrink-0 flex-col">
          <span
            data-part="rating-now"
            className="font-serif text-3xl font-semibold text-ink"
            style={{ fontVariantNumeric: "lining-nums tabular-nums" }}
          >
            {count.format(Math.round(view.chart.current.rating))}
          </span>
          <span className="text-sm text-ink-soft">{tc} rating now</span>
        </p>
        <div className="flex min-w-0 flex-col gap-2">
          <p className="m-0 max-w-prose font-serif text-xl font-semibold text-ink" style={LINING}>
            {view.headline}
          </p>
          <p className="m-0 text-sm text-ink-soft" style={LINING}>
            {view.basis}{" "}
            <Link href={href} className={LINK}>
              See what changed
            </Link>
          </p>
        </div>
      </div>
    </Card>
  );
}
