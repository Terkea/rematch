import type { Db } from "@ci/db";
import { type FormatOption, listFormats } from "../../../../server/filters/formats";
import { defaultRange, type Params, type StatsFilters } from "../../../../server/filters/params";
import { gamesInRange } from "../../../../server/filters/window";
import type { TimeClass } from "../../../../server/games/types";
import { type BarConfig, type FilterBarModel, filterBarModel, parseBarFilters } from "./model";

// One server-side entry for every page's bar (T3): read the class from the URL, list that class's
// formats, then parse the whole contract against them, so a `fmt` from another class is dropped.

export type BarSetup = Omit<BarConfig, "formats" | "now"> & {
  /** The accounts whose formats are listed (the coach scope's one, or all the user's). */
  accountIds: readonly string[];
  now?: Date;
};

export type LoadedBar = { config: BarConfig; filters: StatsFilters; model: FilterBarModel };

export async function loadFilterBar(db: Db, params: Params, setup: BarSetup): Promise<LoadedBar> {
  const { accountIds, now = new Date(), ...rest } = setup;
  const probe: BarConfig = { ...rest, formats: [], now };
  const tc: TimeClass | null = parseBarFilters(params, { ...probe, formatSupported: false }).tc;
  const formats: FormatOption[] =
    tc && setup.formatSupported !== false ? await listFormats(db, accountIds, tc) : [];
  const parsedConfig: BarConfig = { ...probe, formats };
  const filters = parseBarFilters(params, parsedConfig);
  // The default 90 days with too few games say so (ADR 0083 Amendment 2): count them only then.
  const def = defaultRange(parsedConfig);
  const onDefault =
    setup.dates &&
    setup.defaultPeriod !== "all" &&
    !(setup.compare && filters.cmp !== null) &&
    filters.from === def.from &&
    filters.to === def.to;
  const windowGames = onDefault
    ? await gamesInRange(db, accountIds, filters.tc, { from: filters.from, to: filters.to })
    : null;
  const config: BarConfig = { ...parsedConfig, windowGames };
  return { config, filters, model: filterBarModel(filters, config) };
}

/** Next's search params as the contract's `Params` (repeated keys stay arrays, so they fail). */
export const asParams = (p: Record<string, string | string[] | undefined>): Params => p;

/** The coach scope's shape the bar needs (structurally `CoachScope`). */
type ScopeLike = {
  accountId: string;
  timeClass: TimeClass;
  options: readonly { accountId: string; timeClass: TimeClass; analysedGames: number }[];
};

/**
 * The bar of a coach page (progress, weaknesses, areas, report): its classes are the scope
 * account's, every link keeps `account`, and the class shown is always the scope's own (the
 * read models already chose it), so the bar never disagrees with the page.
 */
export function loadCoachBar(
  db: Db,
  params: Record<string, string | string[] | undefined>,
  scope: ScopeLike,
  setup: Omit<BarSetup, "accountIds" | "timeClasses" | "keep" | "allTimeClasses">,
): Promise<LoadedBar> {
  const mine = scope.options.filter((o) => o.accountId === scope.accountId);
  return loadFilterBar(
    db,
    { ...params, tc: scope.timeClass },
    {
      ...setup,
      accountIds: [scope.accountId],
      timeClasses: mine.map((o) => ({ tc: o.timeClass, games: o.analysedGames })),
      keep: { account: scope.accountId },
    },
  );
}

/** More than one linked account among the options: the page keeps its account switch. */
export const manyAccounts = (scope: ScopeLike) =>
  new Set(scope.options.map((o) => o.accountId)).size > 1;
