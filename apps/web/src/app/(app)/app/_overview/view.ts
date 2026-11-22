import type {
  AccuracyTrendPoint,
  AccuracyTrendRolling,
  JudgementColumnsGame,
  ResultSplitRow,
} from "@ci/ui/charts";
import { dayText } from "@ci/ui/charts";
import { accuracyText, gameCaption } from "../../../../components/game-caption";
import type { TimeClass } from "../../../../server/games/types";
import type { Overview } from "../../../../server/overview";
import { CHART_GAMES, type Estimate } from "../../../../server/stats";

// The overview's view model: built on the server from the Overview, holding only what the page
// renders. Whatever reaches a client component (the trend) is plain JSON with no opponents,
// account ids or raw rows (T6 review). Dates are strings.

const TIME_CLASSES: TimeClass[] = ["bullet", "blitz", "rapid", "classical", "daily"];
const CLASS_LABEL: Record<TimeClass, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  daily: "Daily",
};
const PLATFORM = { chesscom: "chess.com", lichess: "Lichess" } as const;

/**
 * The session cookie that says the overview's first-view reveal has run. Here, not in the client
 * module that sets it: a constant from a "use client" module is a client reference on the server.
 */
export const REVEAL_COOKIE = "ci_overview_seen";

/** Games more than this many days apart break the trend's line (no average over empty time). */
export const TREND_GAP_DAYS = 28;

const games = (n: number) => (n === 1 ? "1 game" : `${n} games`);

/** `?tc=`: a class with analysed games, or null (All) for anything else. */
export function parseTimeClass(
  raw: string | string[] | undefined,
  available: readonly TimeClass[],
): TimeClass | null {
  const v = Array.isArray(raw) ? raw[0] : raw;
  const tc = TIME_CLASSES.find((t) => t === v);
  return tc && available.includes(tc) ? tc : null;
}

/** A requested class, before the overview says which classes have analysed games. */
export const knownTimeClass = (raw: string | string[] | undefined): TimeClass | null =>
  parseTimeClass(raw, TIME_CLASSES);

/** The status line, one sentence per entry. */
export function statusText(s: Overview["status"]): string[] {
  if (s.analysed === s.total) return [`${games(s.analysed)} analysed`];
  const lines = [`${s.analysed} of ${games(s.total)} analysed.`];
  if (s.waiting > 0) lines.push(`${s.waiting} waiting for analysis.`);
  if (s.failed > 0) lines.push(`${games(s.failed)} couldn't be analysed.`);
  if (s.skipped > 0) lines.push(`${games(s.skipped)} skipped (variants or too short).`);
  return lines;
}

export type HeaderView = { accounts: { name: string; notes: string[] }[]; status: string[] };

export type EmptyState =
  | { kind: "no-links" }
  | { kind: "no-games" }
  | { kind: "none-analysed"; total: number };

export type SwitchOption = { label: string; count: number; href: string; current: boolean };

export type AccuracyView = {
  points: AccuracyTrendPoint[];
  rolling: AccuracyTrendRolling[];
  /** More analysed games than the chart holds: it shows the latest CHART_GAMES. */
  capped: boolean;
};

export type JudgementsView = {
  basisClass: string;
  games: number;
  rows: {
    key: string;
    label: string;
    estimate: Estimate;
    trend: (number | null)[];
    /** Analysed games behind each weekly point, aligned with `trend`. */
    trendGames: number[];
  }[];
  columns: JudgementColumnsGame[];
};

export type TimeClassView = {
  results: ResultSplitRow[];
  classes: {
    key: TimeClass;
    name: string;
    games: number;
    /** Score as a percent (0 to 100), so it formats as accuracy does. */
    score: Estimate;
    accuracy: Estimate;
  }[];
};

export type RecentRow = {
  id: string;
  href: string;
  caption: string;
  date: string;
  /** The date without its year, for phones ("20 Nov"). */
  dateShort: string;
  result: string;
  opponent: string;
  rating: number | null;
  accuracy: string | null;
};

export type OverviewView = {
  header: HeaderView;
  empty: EmptyState | null;
  switch: SwitchOption[];
  /** The scope's name in sentences: "analysed" for all games, else the class ("rapid"). */
  basisClass: string;
  /** The filtered class's name ("Rapid"), or null for All. */
  scopeName: string | null;
  lead: { accuracy: Estimate; blunders: Estimate };
  accuracy: AccuracyView;
  judgements: JudgementsView;
  byTimeClass: TimeClassView;
  recent: RecentRow[];
};

export const reviewHref = (id: string) => `/app/games/${id}`;

function emptyState(o: Overview): EmptyState | null {
  if (o.accounts.length === 0) return { kind: "no-links" };
  if (o.status.total === 0) return { kind: "no-games" };
  if (o.status.analysed === 0) return { kind: "none-analysed", total: o.status.total };
  return null;
}

function header(o: Overview): HeaderView {
  return {
    accounts: o.accounts.map((a) => {
      const notes: string[] = [];
      if (!a.verified) notes.push("Unverified");
      const last = a.lastImport?.finishedAt ?? a.lastSyncedAt;
      if (last) notes.push(`Last imported ${dayText(last)}`);
      return { name: `${PLATFORM[a.platform]} ${a.username}`, notes };
    }),
    // With no games at all, the empty state says what to do; "0 games analysed" would not.
    status: o.status.total === 0 ? [] : statusText(o.status),
  };
}

/** A rate (0 to 1) as a percent (0 to 100), interval and all. */
function asPercent(e: Estimate): Estimate {
  if (e.kind === "insufficient") return e;
  return {
    ...e,
    value: e.value * 100,
    interval: e.interval && {
      ...e.interval,
      low: e.interval.low * 100,
      high: e.interval.high * 100,
    },
  };
}

const weekly = (o: Overview, key: "blunders" | "mistakes" | "inaccuracies") =>
  o.judgements.weekly.map((w) => w[key]);
const weekGames = (o: Overview) => o.judgements.weekly.map((w) => w.games);

/** The page's view of one Overview, scoped to `tc` (null is All). */
export function toOverviewView(o: Overview, tc: TimeClass | null): OverviewView {
  const empty = emptyState(o);
  const ready = empty === null;
  const basisClass = tc ? CLASS_LABEL[tc].toLowerCase() : "analysed";
  const analysedInScope = tc
    ? (o.timeClasses.find((c) => c.timeClass === tc)?.analysed ?? 0)
    : o.status.analysed;
  const countGames = (e: Estimate) => e.n;

  const switchOptions: SwitchOption[] =
    o.timeClasses.length === 0
      ? []
      : [
          { label: "All", count: o.status.analysed, href: "/app", current: tc === null },
          ...o.timeClasses.map((c) => ({
            label: CLASS_LABEL[c.timeClass],
            count: c.analysed,
            href: `/app?tc=${c.timeClass}`,
            current: tc === c.timeClass,
          })),
        ];

  return {
    header: header(o),
    empty,
    switch: switchOptions,
    basisClass,
    scopeName: tc ? CLASS_LABEL[tc] : null,
    lead: {
      accuracy: o.accuracy.overall,
      blunders: o.judgements.blunders,
    },
    accuracy: ready
      ? {
          points: o.accuracy.points.map((p) => ({
            id: p.id,
            date: p.playedAt.toISOString(),
            accuracy: p.accuracy,
            result: p.result,
            caption: `${dayText(p.playedAt)}, ${p.result}, accuracy ${accuracyText(p.accuracy)}`,
            href: reviewHref(p.id),
          })),
          rolling: o.accuracy.rolling.map((r) => ({
            id: r.id,
            mean: r.mean,
            low: r.low,
            high: r.high,
          })),
          capped: analysedInScope > CHART_GAMES && o.accuracy.points.length >= CHART_GAMES,
        }
      : { points: [], rolling: [], capped: false },
    judgements: {
      basisClass,
      games: countGames(o.judgements.blunders),
      rows: ready
        ? [
            {
              key: "blunders",
              label: "Blunders",
              estimate: o.judgements.blunders,
              trend: weekly(o, "blunders"),
              trendGames: weekGames(o),
            },
            {
              key: "mistakes",
              label: "Mistakes",
              estimate: o.judgements.mistakes,
              trend: weekly(o, "mistakes"),
              trendGames: weekGames(o),
            },
            {
              key: "inaccuracies",
              label: "Inaccuracies",
              estimate: o.judgements.inaccuracies,
              trend: weekly(o, "inaccuracies"),
              trendGames: weekGames(o),
            },
          ]
        : [],
      columns: ready
        ? o.judgements.columns.map((c) => ({
            id: c.id,
            label: `Game of ${dayText(c.playedAt)}`,
            href: reviewHref(c.id),
            blunders: c.blunders,
            mistakes: c.mistakes,
            inaccuracies: c.inaccuracies,
          }))
        : [],
    },
    byTimeClass: ready
      ? {
          results: o.byTimeClass.map((r) => ({
            key: r.timeClass,
            label: CLASS_LABEL[r.timeClass],
            wins: r.wins,
            draws: r.draws,
            losses: r.losses,
          })),
          classes: o.byTimeClass.map((r) => ({
            key: r.timeClass,
            name: CLASS_LABEL[r.timeClass],
            games: r.games,
            score: asPercent(r.score),
            accuracy: r.accuracy,
          })),
        }
      : { results: [], classes: [] },
    recent: ready
      ? o.recent.map((g) => ({
          id: g.id,
          href: reviewHref(g.id),
          caption: gameCaption(g),
          date: dayText(g.playedAt),
          dateShort: dayText(g.playedAt).replace(/ \d{4}$/, ""),
          result: g.result === "unfinished" ? "Unfinished" : CLASS_RESULT[g.result],
          opponent: g.opponent.name,
          rating: g.opponent.rating,
          accuracy: g.accuracy === null ? null : accuracyText(g.accuracy),
        }))
      : [],
  };
}

const CLASS_RESULT = { win: "Win", draw: "Draw", loss: "Loss" } as const;
