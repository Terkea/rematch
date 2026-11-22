"use client";

import { Icon, XIcon } from "@ci/ui/icons";
import { Button, SegmentedControl, Select } from "@ci/ui/primitives";
import { type ReactNode, useOptimistic } from "react";
import type { GameFilters, GameListQuery } from "../../../../../server/games/types";
import { type AccountOption, explorerHref, filterCount, NO_FILTERS } from "./href";
import { useExplorerNavigation } from "./transition";

export type { AccountOption };

const ALL = "all";
const RESULTS = [
  { value: ALL, label: "All" },
  { value: "win", label: "Wins" },
  { value: "draw", label: "Draws" },
  { value: "loss", label: "Losses" },
];
const SORTS = [
  { value: "played-desc", label: "Newest first" },
  { value: "played-asc", label: "Oldest first" },
  { value: "accuracy-desc", label: "Highest accuracy" },
  { value: "accuracy-asc", label: "Lowest accuracy" },
  { value: "opponent-desc", label: "Strongest opponent" },
  { value: "blunders-desc", label: "Most blunders" },
];

/**
 * The explorer's own controls beside the shared filter bar (T3, which holds the time control,
 * colour and dates): the result, the count, Clear filters, the account and the sort. Every change
 * replaces the URL (page reset to 1) inside the shared transition; the control shows its new value
 * at once (optimistic), while the list below dims until the new page arrives. They stay in view
 * at every width.
 */
export function ExplorerControls({
  query: current,
  accounts,
  summary,
}: {
  /** The parsed, validated URL state (never the raw search params). */
  query: GameListQuery;
  accounts: AccountOption[];
  /** The count line ("Showing 1–25 of 500 games"), set beside the sort. */
  summary?: ReactNode;
}) {
  const { start, replace } = useExplorerNavigation();
  const [query, setOptimistic] = useOptimistic(current);
  const f = query.filters;
  const count = filterCount(f);

  const go = (next: GameListQuery) => {
    start(() => {
      setOptimistic(next);
      replace(explorerHref(next));
    });
  };
  const setFilter = (patch: Partial<GameFilters>) =>
    go({ ...query, filters: { ...f, ...patch }, page: 1 });
  const sortValue = `${query.sort.key}-${query.sort.dir}`;

  return (
    <section aria-label="Results and sort" className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-t border-rule pt-4">
        <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
          <SegmentedControl
            label="Result"
            value={f.result ?? ALL}
            options={RESULTS}
            onValueChange={(v) =>
              setFilter({ result: v === ALL ? null : (v as NonNullable<GameFilters["result"]>) })
            }
          />
          <div className="flex min-h-10 flex-wrap items-center gap-x-4 gap-y-1">
            {summary}
            {count > 0 ? (
              <Button
                variant="quiet"
                size="sm"
                onClick={() => go({ ...query, filters: NO_FILTERS, page: 1 })}
              >
                <Icon icon={XIcon} size={16} />
                Clear filters
              </Button>
            ) : null}
          </div>
        </div>
        <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
          {accounts.length > 1 ? (
            <Select
              label="Account"
              className="w-56"
              value={f.accountId ?? ALL}
              options={[
                { value: ALL, label: "All accounts" },
                ...accounts.map((a) => ({ value: a.id, label: a.label })),
              ]}
              onValueChange={(v) => setFilter({ accountId: v === ALL ? null : v })}
            />
          ) : null}
          <Select
            label="Sort by"
            className="w-56"
            value={sortValue}
            options={SORTS}
            onValueChange={(v) => {
              const [key, dir] = v.split("-") as [GameListQuery["sort"]["key"], "asc" | "desc"];
              go({ ...query, sort: { key, dir }, page: 1 });
            }}
          />
        </div>
      </div>
    </section>
  );
}
