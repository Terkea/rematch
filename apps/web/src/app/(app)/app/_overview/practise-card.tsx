import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import type { PractiseView } from "./practise-view";

// The overview's one call to action (ADR 0092): today's trainer session as a single primary
// button, "All done for today" with the next due day, or "Start training" on a first run. A plain
// sheet like the focus card above it (no coloured border, B10).

const LINK =
  "rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink";
const NUMS = "[font-variant-numeric:lining-nums]";

export function PractiseCard({ view }: { view: PractiseView }) {
  return (
    <section
      aria-labelledby="practise-heading"
      data-slot="practise"
      data-practise={view.kind}
      data-surface="sheet"
      className="flex max-w-3xl flex-col gap-3 rounded-sheet bg-sheet px-(--space-panel-pad) py-5"
    >
      <h2 id="practise-heading" className="m-0 text-base font-medium text-ink-soft">
        Practise
      </h2>
      {view.kind === "done" ? (
        <>
          <p className={`m-0 font-serif text-xl font-semibold text-ink ${NUMS}`}>{view.line}</p>
          <p className={`m-0 max-w-prose text-sm text-ink-soft ${NUMS}`}>
            {view.detail ? `${view.detail} ` : ""}
            <Link href={view.href} className={LINK}>
              Open the trainer
            </Link>
          </p>
        </>
      ) : (
        <>
          <Button asChild variant="primary" size="md" className={`w-fit ${NUMS}`}>
            <Link href={view.href}>{view.action}</Link>
          </Button>
          <p className={`m-0 max-w-prose text-sm text-ink-soft ${NUMS}`}>{view.detail}</p>
        </>
      )}
    </section>
  );
}
