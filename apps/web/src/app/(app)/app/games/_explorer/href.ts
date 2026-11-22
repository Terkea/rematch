import { dayText } from "@ci/ui/charts";
import { NO_FILTERS, serializeGameQuery } from "../../../../../server/games/query";
import type { GameFilters, GameListQuery, TimeClass } from "../../../../../server/games/types";

export { NO_FILTERS };

/** An account the user can narrow to: its id and "chess.com name". */
export type AccountOption = { id: string; label: string };

/** `/app/games` plus the query, with nothing after it when every value is a default. */
export function explorerHref(q: GameListQuery): string {
  const qs = serializeGameQuery(q).toString();
  return qs ? `/app/games?${qs}` : "/app/games";
}

/** How many filters are set (the sort is not a filter). */
export function filterCount(f: GameFilters): number {
  return Object.values(f).filter((v) => v !== null).length;
}

export const CLASS_NAME: Record<TimeClass, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  daily: "Daily",
};
const RESULT_PLURAL = { win: "wins", draw: "draws", loss: "losses" } as const;
const RESULT_CHIP = { win: "Wins", draw: "Draws", loss: "Losses" } as const;
const COLOUR = { white: "White", black: "Black" } as const;

/** "1 Oct 2026" for a `YYYY-MM-DD` day. */
export const dayWords = (day: string) => dayText(new Date(`${day}T00:00:00Z`));

const accountLabel = (id: string, accounts: AccountOption[]) =>
  accounts.find((a) => a.id === id)?.label ?? "that account";

/**
 * The filters in words, for the no-match state: "Classical draws as White on chess.com name
 * between 1 Oct 2026 and 20 Nov 2026".
 */
export function describeFilters(f: GameFilters, accounts: AccountOption[]): string {
  const noun = f.result ? RESULT_PLURAL[f.result] : "games";
  let out = f.timeClass ? `${CLASS_NAME[f.timeClass]} ${noun}` : noun;
  if (f.colour) out += ` as ${COLOUR[f.colour]}`;
  if (f.accountId) out += ` on ${accountLabel(f.accountId, accounts)}`;
  if (f.from && f.to) out += ` between ${dayWords(f.from)} and ${dayWords(f.to)}`;
  else if (f.from) out += ` since ${dayWords(f.from)}`;
  else if (f.to) out += ` up to ${dayWords(f.to)}`;
  return out;
}

/** The set filters as chips, in the URL's order, each with the filter it clears. */
export function activeFilters(
  f: GameFilters,
  accounts: AccountOption[],
): { label: string; clear: Partial<GameFilters> }[] {
  const out: { label: string; clear: Partial<GameFilters> }[] = [];
  if (f.timeClass) out.push({ label: CLASS_NAME[f.timeClass], clear: { timeClass: null } });
  if (f.result) out.push({ label: RESULT_CHIP[f.result], clear: { result: null } });
  if (f.colour) out.push({ label: COLOUR[f.colour], clear: { colour: null } });
  if (f.from) out.push({ label: `From ${dayWords(f.from)}`, clear: { from: null } });
  if (f.to) out.push({ label: `To ${dayWords(f.to)}`, clear: { to: null } });
  if (f.accountId)
    out.push({ label: accountLabel(f.accountId, accounts), clear: { accountId: null } });
  return out;
}

const RELAX_LABEL = {
  timeClass: "Any time control",
  result: "Any result",
  colour: "Either colour",
  dates: "Any date",
  from: "Any date",
  to: "Any date",
  accountId: "All accounts",
} as const;

/** The no-match state's ways out: each filter dropped, with its count and link. */
export function suggestions(
  query: GameListQuery,
  relax: { key: keyof GameFilters | "dates"; total: number }[],
): { label: string; total: number; href: string }[] {
  return relax.map(({ key, total }) => {
    const filters =
      key === "dates"
        ? { ...query.filters, from: null, to: null }
        : { ...query.filters, [key]: null };
    return {
      label: RELAX_LABEL[key],
      total,
      href: explorerHref({ ...query, filters, page: 1 }),
    };
  });
}
