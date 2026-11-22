import type { ClassCount } from "../../../../../server/games/explorer";
import { serializeGameQuery } from "../../../../../server/games/query";
import type { GameListQuery } from "../../../../../server/games/types";
import { type BarConfig, type FilterBarModel, filterBarModel } from "../../_filters/model";

// The games page's part of the shared filter bar (T3): time control (with All), colour and dates,
// read from the explorer's own query so the two never disagree. The explorer's other params
// (result, account, sort) ride along on every link; the page goes back to 1.

const KEEP = ["result", "account", "sort"] as const;

export function gamesBarConfig(query: GameListQuery, classes: ClassCount[], now: Date): BarConfig {
  const own = serializeGameQuery(query);
  const keep: Record<string, string> = {};
  for (const k of KEEP) {
    const v = own.get(k);
    if (v !== null) keep[k] = v;
  }
  return {
    basePath: "/app/games",
    timeClasses: classes,
    allTimeClasses: true,
    formats: [],
    formatSupported: false,
    color: "on",
    dates: true,
    compare: false,
    keep,
    now,
  };
}

export function gamesBarModel(
  query: GameListQuery,
  classes: ClassCount[],
  now: Date,
): FilterBarModel {
  const f = query.filters;
  return filterBarModel(
    {
      tc: f.timeClass,
      fmt: null,
      from: f.from,
      to: f.to,
      color: f.colour ?? "both",
      cmp: null,
      a: null,
      b: null,
    },
    gamesBarConfig(query, classes, now),
  );
}
