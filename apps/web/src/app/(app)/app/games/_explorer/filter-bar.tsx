"use client";

import { FunnelSimpleIcon, Icon, XIcon } from "@ci/ui/icons";
import { Button, Field, Input, SegmentedControl, Select, touchTarget } from "@ci/ui/primitives";
import { type ReactNode, useEffect, useId, useOptimistic, useRef, useState } from "react";
import { parseGameQuery } from "../../../../../server/games/query";
import type { GameFilters, GameListQuery } from "../../../../../server/games/types";
import { type AccountOption, activeFilters, explorerHref, filterCount, NO_FILTERS } from "./href";
import { useExplorerNavigation } from "./transition";

export type { AccountOption };

const ALL = "all";
const TIME_CONTROLS = [
  { value: ALL, label: "All time controls" },
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
  { value: "classical", label: "Classical" },
  { value: "daily", label: "Daily" },
];
const RESULTS = [
  { value: ALL, label: "All" },
  { value: "win", label: "Wins" },
  { value: "draw", label: "Draws" },
  { value: "loss", label: "Losses" },
];
const COLOURS = [
  { value: ALL, label: "Both" },
  { value: "white", label: "White" },
  { value: "black", label: "Black" },
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
 * A date filter. The field keeps its own value while it is typed in, and navigates only once the
 * value is a day the URL accepts (so a year typed digit by digit, `0002`, never navigates). It is
 * never remounted, so focus stays put when the new page arrives.
 */
function DateFilter({
  label,
  value,
  min,
  max,
  onCommit,
}: {
  label: string;
  value: string | null;
  min?: string | null;
  max?: string | null;
  onCommit: (day: string | null) => void;
}) {
  const [text, setText] = useState(value ?? "");
  useEffect(() => setText(value ?? ""), [value]);
  return (
    <Field label={label} className="md:w-40">
      <Input
        type="date"
        value={text}
        min={min ?? undefined}
        max={max ?? undefined}
        onChange={(e) => {
          const v = e.currentTarget.value;
          setText(v);
          if (v === "") onCommit(null);
          else if (parseGameQuery({ from: v }).filters.from === v) onCommit(v);
        }}
      />
    </Field>
  );
}

/**
 * The explorer's filters, sort and account. Every change replaces the URL (page reset to 1)
 * inside the shared transition; the control shows its new value at once (optimistic), while the
 * list below dims until the new page arrives. Below `md` the filters fold behind one button, and
 * the active ones show as removable chips; the count, Clear filters, Account and the sort stay in
 * view at every width.
 */
export function FilterBar({
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
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const f = query.filters;
  const count = filterCount(f);
  const chips = activeFilters(f, accounts);
  const toggleRef = useRef<HTMLElement>(null);
  const chipsRef = useRef<HTMLUListElement>(null);
  // After a chip is removed and the new filters arrive, focus the chip that took its place (or
  // the one before it at the end), or the Filters button when none is left, never <body>.
  const refocus = useRef<number | null>(null);
  const chipCount = chips.length;
  useEffect(() => {
    const at = refocus.current;
    if (at === null || chipCount === 0) {
      if (at !== null) toggleRef.current?.focus();
      refocus.current = null;
      return;
    }
    const buttons = chipsRef.current?.querySelectorAll<HTMLButtonElement>("button");
    buttons?.[Math.min(at, chipCount - 1)]?.focus();
    refocus.current = null;
  }, [chipCount]);

  const go = (next: GameListQuery) => {
    start(() => {
      setOptimistic(next);
      replace(explorerHref(next));
    });
  };
  const setFilter = (patch: Partial<GameFilters>) =>
    go({ ...query, filters: { ...f, ...patch }, page: 1 });
  const fromAll = <T extends string>(v: string) => (v === ALL ? null : (v as T));
  const sortValue = `${query.sort.key}-${query.sort.dir}`;

  return (
    <section aria-label="Filters" className="flex flex-col gap-4">
      <div className="flex flex-col gap-3 md:hidden">
        <Button
          ref={toggleRef}
          variant="outline"
          className="self-start"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={() => setOpen((o) => !o)}
        >
          <Icon icon={FunnelSimpleIcon} size={20} />
          {count > 0 ? `Filters (${count})` : "Filters"}
        </Button>
        {chips.length > 0 ? (
          <ul ref={chipsRef} aria-label="Active filters" className="flex flex-wrap gap-3">
            {chips.map((c, i) => (
              <li key={c.label}>
                <button
                  type="button"
                  aria-label={`Remove filter: ${c.label}`}
                  onClick={() => {
                    refocus.current = i;
                    setFilter(c.clear);
                  }}
                  className={`relative inline-flex h-8 items-center gap-1.5 rounded-pill bg-well py-1 pr-2.5 pl-3 text-sm font-medium text-ink inset-ring-1 inset-ring-rule hover:bg-sheet focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink ${touchTarget}`}
                >
                  {c.label}
                  <Icon icon={XIcon} size={16} className="text-ink-soft" />
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </div>
      <div
        id={panelId}
        data-open={open ? "" : undefined}
        className="hidden flex-col gap-4 data-open:flex md:flex md:flex-row md:flex-wrap md:items-end md:gap-x-6 md:gap-y-4"
      >
        <Select
          label="Time control"
          className="md:w-44"
          value={f.timeClass ?? ALL}
          options={TIME_CONTROLS}
          onValueChange={(v) =>
            setFilter({ timeClass: fromAll<NonNullable<GameFilters["timeClass"]>>(v) })
          }
        />
        <SegmentedControl
          label="Result"
          value={f.result ?? ALL}
          options={RESULTS}
          onValueChange={(v) =>
            setFilter({ result: fromAll<NonNullable<GameFilters["result"]>>(v) })
          }
        />
        <SegmentedControl
          label="Colour"
          value={f.colour ?? ALL}
          options={COLOURS}
          onValueChange={(v) =>
            setFilter({ colour: fromAll<NonNullable<GameFilters["colour"]>>(v) })
          }
        />
        <div className="grid grid-cols-2 gap-3 md:flex md:gap-3">
          <DateFilter
            label="From"
            value={f.from}
            max={f.to}
            onCommit={(from) => setFilter({ from })}
          />
          <DateFilter label="To" value={f.to} min={f.from} onCommit={(to) => setFilter({ to })} />
        </div>
      </div>
      <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-t border-rule pt-4">
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
