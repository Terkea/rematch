import { Button } from "@ci/ui/primitives";
import { ProgressPlaceholder } from "@ci/ui/progress";
import Link from "next/link";

// The explorer's own empty states. The no-links copy is Task 8's, word for word; the overview's
// `OverviewEmpty` (m5a/t8) draws a graph-shaped placeholder and has no "import finished, still
// empty" case, so the list keeps its own (see the T9 report).

const SHEET =
  "flex flex-col items-start gap-4 rounded-sheet bg-sheet px-(--space-panel-pad) py-8 md:py-10";

/** No account linked yet: one line and the way to fix it. */
export function NoLinks() {
  return (
    <div data-empty="no-links" data-surface="sheet" className={SHEET}>
      <p className="max-w-prose font-serif text-xl font-semibold text-ink">
        Connect a chess.com account to see your games here.
      </p>
      <p className="max-w-prose text-base text-ink-soft">
        Your games are imported once it's linked, then each one is analysed move by move.
      </p>
      <Button asChild variant="outline">
        <Link href="/app/accounts">Connect chess.com</Link>
      </Button>
    </div>
  );
}

/**
 * Linked, but no games. While an import is due or running, the list's shape stands in with what
 * is happening. A failed or stuck import says so and points to Accounts; a finished one says the
 * account is empty. Never "importing" forever.
 */
export function NoGames({
  importState,
  accounts,
}: {
  importState: "importing" | "failed" | "done";
  accounts: string[];
}) {
  const names = accounts.join(" and ");
  if (importState === "failed") {
    return (
      <div data-empty="import-failed" data-surface="sheet" className={SHEET}>
        <p className="max-w-prose font-serif text-xl font-semibold text-ink">
          Import didn't finish. Try again from Accounts.
        </p>
        <p className="max-w-prose text-base text-ink-soft">
          No games from {names} have arrived yet.
        </p>
        <Button asChild variant="outline">
          <Link href="/app/accounts">Go to Accounts</Link>
        </Button>
      </div>
    );
  }
  if (importState === "importing") {
    return (
      <div data-empty="importing" className="flex flex-col gap-4">
        <p className="max-w-prose text-base text-ink-soft">
          Importing your games from {names}. They appear here as they arrive, usually within a
          minute or two, and each is then analysed.
        </p>
        <ProgressPlaceholder shape="list" verb="Importing" />
      </div>
    );
  }
  return (
    <div data-empty="no-games" data-surface="sheet" className={SHEET}>
      <p className="max-w-prose font-serif text-xl font-semibold text-ink">
        {accounts.length === 1 ? `${names} has no games yet.` : `${names} have no games yet.`}
      </p>
      <p className="max-w-prose text-base text-ink-soft">
        Games you finish there appear here after the next import.
      </p>
      <Button asChild variant="outline">
        <Link href="/app/accounts">Go to Accounts</Link>
      </Button>
    </div>
  );
}

/**
 * The filters matched nothing: what was asked for, in words, the nearest wider lists with their
 * counts, and the way back to everything.
 */
export function NoMatch({
  description,
  suggestions,
  clearHref,
}: {
  description: string;
  suggestions: { label: string; total: number; href: string }[];
  clearHref: string;
}) {
  return (
    <div data-empty="no-match" data-surface="sheet" className={SHEET}>
      <div className="flex flex-col gap-1.5">
        <p className="font-serif text-xl font-semibold text-ink">No games match these filters.</p>
        <p className="max-w-prose text-base text-ink-soft">You asked for {description}.</p>
      </div>
      {suggestions.length > 0 ? (
        <div className="flex flex-col gap-2">
          <p className="text-sm text-ink-soft">Widen it:</p>
          <ul className="flex flex-wrap gap-2">
            {suggestions.map((s) => (
              <li key={s.href}>
                <Button asChild variant="outline" size="sm">
                  <Link href={s.href} scroll={false}>
                    {`${s.label}: ${s.total} ${s.total === 1 ? "game" : "games"}`}
                  </Link>
                </Button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      <Button asChild variant="outline">
        <Link href={clearHref} scroll={false}>
          Clear filters
        </Link>
      </Button>
    </div>
  );
}
