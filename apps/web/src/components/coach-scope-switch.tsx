"use client";

import { SegmentedControl, Select } from "@ci/ui/primitives";
import { useRouter } from "next/navigation";
import { byTimeClass } from "../lib/time-class";
import type { TimeClass } from "../server/games/types";

type Platform = "lichess" | "chesscom";

/** One choosable scope; structurally Task 1's `CoachScope["options"][number]`. */
export type CoachScopeOption = {
  accountId: string;
  platform: Platform;
  username: string;
  timeClass: TimeClass;
  analysedGames: number;
};

/** Structurally Task 1's `CoachScope`. */
export type CoachScopeLike = {
  accountId: string;
  timeClass: TimeClass;
  platform: Platform;
  username: string;
  options: CoachScopeOption[];
};

const TIME_CLASS: Record<TimeClass, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  daily: "Daily",
};
const PLATFORM: Record<Platform, string> = { lichess: "lichess", chesscom: "chess.com" };

/** Up to this many options show as a segmented control; more become a select. */
const SEGMENTED_MAX = 3;

const count = new Intl.NumberFormat("en-GB");

/** "Rapid on lichess (214 games)"; with `withUsername` "… as name (…)". */
export function scopeOptionLabel(o: CoachScopeOption, withUsername: boolean): string {
  const who = withUsername ? ` as ${o.username}` : "";
  const n = Math.max(0, Math.trunc(o.analysedGames));
  const games = `${count.format(n)} ${n === 1 ? "game" : "games"}`;
  return `${TIME_CLASS[o.timeClass]} on ${PLATFORM[o.platform]}${who} (${games})`;
}

const scopeValue = (o: { accountId: string; timeClass: TimeClass }) =>
  `${o.accountId}:${o.timeClass}`;

/**
 * The coach pages' scope switch (R1): which linked account and time class the page coaches.
 * Writes `account` and `tc` to `basePath`'s URL. Renders nothing when there is one option.
 */
export function CoachScopeSwitch({ scope, basePath }: { scope: CoachScopeLike; basePath: string }) {
  const router = useRouter();
  if (scope.options.length <= 1) return null;
  // Two accounts on one platform are told apart by username.
  const perPlatform = new Map<Platform, Set<string>>();
  for (const o of scope.options) {
    const set = perPlatform.get(o.platform) ?? new Set<string>();
    set.add(o.accountId);
    perPlatform.set(o.platform, set);
  }
  // Rapid first, then blitz, then the rest (I8), whatever order the options came in.
  const ordered = [...scope.options].sort((a, b) => byTimeClass(a.timeClass, b.timeClass));
  const options = ordered.map((o) => ({
    value: scopeValue(o),
    label: scopeOptionLabel(o, (perPlatform.get(o.platform)?.size ?? 0) > 1),
  }));
  const choose = (value: string) => {
    const o = scope.options.find((x) => scopeValue(x) === value);
    if (!o || value === scopeValue(scope)) return;
    const q = new URLSearchParams({ account: o.accountId, tc: o.timeClass });
    router.push(`${basePath}?${q.toString()}`, { scroll: false });
  };
  const props = {
    label: "Games coached",
    value: scopeValue(scope),
    options,
    onValueChange: choose,
  };
  return options.length <= SEGMENTED_MAX ? <SegmentedControl {...props} /> : <Select {...props} />;
}
