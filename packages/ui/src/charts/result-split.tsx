"use client";

import { useId } from "react";
import { cn } from "../lib/index.js";
import { ChartPatterns, patternFill } from "./patterns.js";
import { resultText, share, whole } from "./scales.js";

export interface ResultSplitRow {
  key: string;
  /** The row's name, for example the time control "Rapid". */
  label: string;
  wins: number;
  draws: number;
  losses: number;
}

export interface ResultSplitProps {
  /**
   * One row per group. Counts are taken as whole games (negative or fractional counts read as
   * whole games only). Rows with no games are skipped, and with none at all the component
   * renders nothing: the host owns that empty state.
   */
  rows: ResultSplitRow[];
  className?: string;
}

const SEGMENTS = ["wins", "draws", "losses"] as const;

/**
 * Wins, draws and losses as one 100% bar per row. The words always carry the counts ("8 wins,
 * 2 draws, 4 losses"), and the bar tells the three apart by fill, not colour: solid `ink`, `ink`
 * hatch, and an outlined `sheet` segment. A row with no games is not drawn: an empty bar would
 * claim a split that has no evidence.
 */
export function ResultSplit({ rows, className }: ResultSplitProps) {
  const prefix = `rs${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const shown = rows
    .map((r) => ({ ...r, wins: whole(r.wins), draws: whole(r.draws), losses: whole(r.losses) }))
    .filter((r) => r.wins + r.draws + r.losses > 0);
  if (shown.length === 0) return null;

  return (
    <div className={cn("relative", className)}>
      <ChartPatterns prefix={prefix} />
      <ul className="m-0 flex list-none flex-col gap-4 p-0">
        {shown.map((row) => {
          const total = row.wins + row.draws + row.losses;
          const text = resultText(row.wins, row.draws, row.losses);
          return (
            <li key={row.key} className="flex flex-col gap-1.5">
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-0.5">
                <span className="font-sans text-sm font-medium text-ink">{row.label}</span>
                <span className="text-sm text-ink-soft">{text}</span>
              </div>
              {/* The words above are the accessible text; the bar repeats them as a drawing. */}
              <div aria-hidden="true" className="-mx-px flex h-3">
                {SEGMENTS.map((segment) => {
                  const count = row[segment];
                  if (count === 0) return null;
                  return (
                    <div
                      key={segment}
                      data-segment={segment}
                      // 1px each side: neighbouring segments get a 2px gap of the surface.
                      className="h-full px-px"
                      style={{ width: `${share(count, total)}%` }}
                    >
                      {segment === "draws" ? (
                        <svg
                          data-fill
                          className="block size-full rounded-board bg-sheet"
                          role="none"
                        >
                          <rect
                            width="100%"
                            height="100%"
                            fill={patternFill(prefix, "hatch-ink")}
                          />
                        </svg>
                      ) : (
                        <div
                          data-fill
                          className={cn(
                            "size-full rounded-board",
                            segment === "wins" ? "bg-ink" : "border border-rule-strong bg-sheet",
                          )}
                        />
                      )}
                    </div>
                  );
                })}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
