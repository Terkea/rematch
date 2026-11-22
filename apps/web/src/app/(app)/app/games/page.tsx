import { redirect } from "next/navigation";
import { LiveRefresh } from "../../../../components/live-refresh";
import { SyncPanel } from "../../../../components/sync/sync-panel";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { scopedAccountIds } from "../../../../server/games";
import { loadExplorer } from "../../../../server/games/explorer";
import { colourRedirect } from "../../../../server/games/query";
import { getSyncStatus } from "../../../../server/sync";
import { PageHeading } from "../../_shell/page-heading";
import { FilterBar } from "../_filters/filter-bar";
import { NoGames, NoLinks, NoMatch } from "./_explorer/empty";
import { ExplorerControls } from "./_explorer/filter-bar";
import { toRowView } from "./_explorer/game-row";
import { GamesList, showingText } from "./_explorer/games-list";
import { describeFilters, explorerHref, NO_FILTERS, suggestions } from "./_explorer/href";
import { Pagination } from "./_explorer/pagination";
import { gamesBarModel } from "./_explorer/shared-bar";
import { ExplorerTransition, PendingRegion } from "./_explorer/transition";

export const dynamic = "force-dynamic";
export const metadata = { title: "Games" };

type Params = Record<string, string | string[] | undefined>;

/**
 * The games explorer: every game of the user's linked accounts, filtered, sorted and paged by the
 * URL (ADR 0048). Rows link to the review. The filter bar and the list share one transition, so
 * a change dims the list in place until the new page arrives.
 */
export default async function GamesPage({ searchParams }: { searchParams: Promise<Params> }) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  // The shared filter contract names it `color` (ADR 0083); an old `colour` link moves over.
  const moved = colourRedirect("/app/games", params);
  if (moved) redirect(moved);
  const explorer = await loadExplorer(db, user.id, params);
  const live = <LiveRefresh accountIds={await scopedAccountIds(db, user.id)} />;
  const status = await getSyncStatus(db, user.id);
  // "Sync now" and the analysis progress per account (Task P2); nothing without a link.
  const sync = status.accounts.length > 0 ? <SyncPanel initial={status} variant="full" /> : null;

  if (explorer.state !== "games") {
    return (
      <div className="flex flex-col gap-8">
        {live}
        <PageHeading title="Games" />
        {explorer.state === "no-links" ? (
          <NoLinks />
        ) : (
          <NoGames
            importState={explorer.importState}
            accounts={explorer.accounts.map((a) => a.label)}
          />
        )}
        {sync}
      </div>
    );
  }

  const { query, page, accounts } = explorer;
  const rows = page.rows.map(toRowView);
  const clearHref = explorerHref({ ...query, filters: NO_FILTERS, page: 1 });

  return (
    <ExplorerTransition>
      <div className="flex flex-col gap-6 md:gap-8">
        {live}
        <PageHeading title="Games" />
        {sync}
        <FilterBar model={gamesBarModel(query, explorer.classes, new Date())} />
        <ExplorerControls
          query={query}
          accounts={accounts}
          summary={
            <p aria-live="polite" className="text-sm text-ink-soft tabular-nums">
              {page.total === 0 ? "Showing 0 games" : showingText(page)}
            </p>
          }
        />
        <PendingRegion>
          {rows.length === 0 ? (
            <NoMatch
              description={describeFilters(query.filters, accounts)}
              suggestions={suggestions(query, explorer.relax ?? [])}
              clearHref={clearHref}
            />
          ) : (
            <GamesList rows={rows} grouped={query.sort.key === "played"} />
          )}
          <Pagination query={query} page={page.page} pageCount={page.pageCount} />
        </PendingRegion>
      </div>
    </ExplorerTransition>
  );
}
