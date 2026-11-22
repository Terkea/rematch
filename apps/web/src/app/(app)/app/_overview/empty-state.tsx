import { Button } from "@ci/ui/primitives";
import { ProgressPlaceholder } from "@ci/ui/progress";
import Link from "next/link";
import type { EmptyState } from "./view";

/** The three states with nothing to chart: no link, a link with no games, nothing analysed. */
export function OverviewEmpty({ state }: { state: EmptyState }) {
  if (state.kind === "no-links") {
    return (
      <div
        data-empty="no-links"
        data-surface="sheet"
        className="flex flex-col items-center gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-12 text-center"
      >
        <p className="max-w-md text-base text-ink-soft">
          Connect a chess.com account to see your games here.
        </p>
        <Button asChild variant="outline">
          <Link href="/app/accounts">Connect chess.com</Link>
        </Button>
      </div>
    );
  }
  if (state.kind === "no-games") {
    return (
      <div data-empty="no-games">
        <ProgressPlaceholder shape="graph" verb="Importing" />
      </div>
    );
  }
  return (
    <div data-empty="none-analysed">
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 0, total: state.total, unit: "games" }}
      />
    </div>
  );
}
