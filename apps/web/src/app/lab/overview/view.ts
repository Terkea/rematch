import type {
  AccuracyTrendPoint,
  AccuracyTrendRolling,
  JudgementColumnsGame,
  ResultSplitRow,
} from "@ci/ui/charts";
import { dayText } from "@ci/ui/charts";
import type { GameRow, TimeClass } from "../../../server/games/types";
import type { Overview } from "../../../server/overview";
import type { Estimate } from "../../../server/stats";

// Lab only: the pure mapping from the read model to the charts and rows. Task 8 moves what the
// chosen layout needs into the page (`EstimateKpi`, `gameCaption`).

type Status = Overview["status"];

const games = (n: number) => (n === 1 ? "1 game" : `${n} games`);

/** The status line, one sentence per entry (Task 8's copy). */
export function statusText(s: Status): string[] {
  if (s.analysed === s.total) return [`${games(s.analysed)} analysed`];
  const lines = [`${s.analysed} of ${games(s.total)} analysed.`];
  if (s.waiting > 0) lines.push(`${s.waiting} waiting for analysis.`);
  if (s.failed > 0) lines.push(`${games(s.failed)} couldn't be analysed.`);
  if (s.skipped > 0) lines.push(`${games(s.skipped)} skipped (variants or too short).`);
  return lines;
}

const PLATFORM = { chesscom: "chess.com", lichess: "Lichess" } as const;

export function accountText(a: Overview["accounts"][number]): { name: string; notes: string[] } {
  const notes: string[] = [];
  if (!a.verified) notes.push("Unverified");
  const last = a.lastImport?.finishedAt ?? a.lastSyncedAt;
  if (last) notes.push(`Last imported ${dayText(last)}`);
  return { name: `${PLATFORM[a.platform]} ${a.username}`, notes };
}

/** `percent` is 0 to 100 (accuracy), `rate` is 0 to 1 (score), `decimal` is per game. */
export type EstimateFormat = "percent" | "rate" | "decimal";
type Basis = { games: number; timeClass: string };
type Band = { low: number; high: number; level: 0.8 };

export type EstimateView =
  | { kind: "kpi"; value: number; interval: Band; basis: Basis }
  | { kind: "flat"; value: number; note: string; basis: Basis }
  | { kind: "insufficient"; need: string; basis: string };

export const basisLine = (n: number, timeClass: string) =>
  `Based on ${n} ${timeClass} ${n === 1 ? "game" : "games"}`;

/**
 * What a KPI row may honestly show: a value with its band, a value with no band (no spread, so
 * no interval is estimable; never widened), or no value at all below the minimum.
 */
export function estimateView(
  e: Estimate,
  format: EstimateFormat,
  basisClass: string,
): EstimateView {
  if (e.kind === "insufficient") {
    return {
      kind: "insufficient",
      need: `Need ${e.needed} more ${e.needed === 1 ? "game" : "games"}`,
      basis: basisLine(e.n, basisClass),
    };
  }
  const k = format === "percent" ? 100 : 1;
  const basis = { games: e.n, timeClass: basisClass };
  if (!e.interval) {
    return {
      kind: "flat",
      value: e.value / k,
      note: "Every game gave the same value, so there is no interval",
      basis,
    };
  }
  return {
    kind: "kpi",
    value: e.value / k,
    interval: { low: e.interval.low / k, high: e.interval.high / k, level: 0.8 },
    basis,
  };
}

export const reviewHref = (id: string) => `/app/games/${id}`;

const pct = (v: number) => `${(Math.round(v * 10) / 10).toFixed(1)}%`;

export function trendPoints(points: Overview["accuracy"]["points"]): AccuracyTrendPoint[] {
  return points.map((p) => ({
    id: p.id,
    date: p.playedAt.toISOString(),
    accuracy: p.accuracy,
    result: p.result,
    caption: `${dayText(p.playedAt)}, ${p.result}, accuracy ${pct(p.accuracy)}`,
    href: reviewHref(p.id),
  }));
}

/** The latest `n` games (points are oldest first) and the rolling entries that end on them. */
export function latest<P extends { id: string }, R extends { id: string }>(
  points: P[],
  rolling: R[],
  n: number,
): { points: P[]; rolling: R[] } {
  const kept = points.slice(Math.max(0, points.length - n));
  const ids = new Set(kept.map((p) => p.id));
  return { points: kept, rolling: rolling.filter((r) => ids.has(r.id)) };
}

export function columnGames(columns: Overview["judgements"]["columns"]): JudgementColumnsGame[] {
  return columns.map((c) => ({
    id: c.id,
    label: `Game of ${dayText(c.playedAt)}`,
    href: reviewHref(c.id),
    blunders: c.blunders,
    mistakes: c.mistakes,
    inaccuracies: c.inaccuracies,
  }));
}

const CLASS_LABEL: Record<TimeClass, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  daily: "Daily",
};
export const classLabel = (tc: TimeClass) => CLASS_LABEL[tc];

export function splitRows(rows: Overview["byTimeClass"]): ResultSplitRow[] {
  return rows.map((r) => ({
    key: r.timeClass,
    label: classLabel(r.timeClass),
    wins: r.wins,
    draws: r.draws,
    losses: r.losses,
  }));
}

/** One weekly series for a sparkline, oldest first; a week without games is a gap. */
export const weeklySeries = (
  weekly: Overview["judgements"]["weekly"],
  key: "blunders" | "mistakes" | "inaccuracies" | "accuracy",
): (number | null)[] =>
  weekly.map((w) => {
    const v = w[key];
    if (v === null) return null;
    return key === "accuracy" ? v / 100 : v;
  });

/** "12 Nov 2026, win v Opponent (1520), accuracy 71.3%" (Task 8's `gameCaption`). */
export function recentCaption(row: GameRow): string {
  const rating = row.opponent.rating === null ? "" : ` (${row.opponent.rating})`;
  const acc = row.accuracy === null ? "" : `, accuracy ${pct(row.accuracy)}`;
  return `${dayText(row.playedAt)}, ${row.result} v ${row.opponent.name}${rating}${acc}`;
}

export type RollingEntry = AccuracyTrendRolling;

// The narrow view models for the client variants (b and c). Whatever a client component gets is
// serialised into the page, so these hold only what is rendered: no raw rows, no opponents, no
// account ids, and dates as strings. Built on the server; tested by lab-overview-boundary.

export type EmptyState =
  | { kind: "no-links" }
  | { kind: "no-games" }
  | { kind: "none-analysed"; total: number };

/** The state with nothing to chart (Task 8's three), or null when there is something. */
export function emptyState(o: Overview): EmptyState | null {
  if (o.accounts.length === 0) return { kind: "no-links" };
  if (o.status.total === 0) return { kind: "no-games" };
  if (o.status.analysed === 0) return { kind: "none-analysed", total: o.status.total };
  return null;
}

export type HeaderView = { accounts: { name: string; notes: string[] }[]; status: string[] };

export function headerView(o: Overview): HeaderView {
  return { accounts: o.accounts.map(accountText), status: statusText(o.status) };
}

export type KpiRowView = {
  key: string;
  label: string;
  format: "percent" | "decimal";
  view: EstimateView;
  trend?: (number | null)[];
};

function kpiRow(
  key: string,
  label: string,
  estimate: Estimate,
  format: EstimateFormat,
  basisClass: string,
  trend?: (number | null)[],
): KpiRowView {
  return {
    key,
    label,
    format: format === "decimal" ? "decimal" : "percent",
    view: estimateView(estimate, format, basisClass),
    ...(trend ? { trend } : {}),
  };
}

export type TrendView = { points: AccuracyTrendPoint[]; rolling: AccuracyTrendRolling[] };

const trendView = (o: Overview): TrendView => ({
  points: trendPoints(o.accuracy.points),
  rolling: o.accuracy.rolling.map((r) => ({ id: r.id, mean: r.mean, low: r.low, high: r.high })),
});

/** Accuracy, then blunders, mistakes and inaccuracies per game, each with its weekly series. */
function perGameRows(o: Overview, basisClass: string, accuracyLabel: string): KpiRowView[] {
  const w = o.judgements.weekly;
  return [
    kpiRow(
      "accuracy",
      accuracyLabel,
      o.accuracy.overall,
      "percent",
      basisClass,
      weeklySeries(w, "accuracy"),
    ),
    kpiRow(
      "blunders",
      "Blunders",
      o.judgements.blunders,
      "decimal",
      basisClass,
      weeklySeries(w, "blunders"),
    ),
    kpiRow(
      "mistakes",
      "Mistakes",
      o.judgements.mistakes,
      "decimal",
      basisClass,
      weeklySeries(w, "mistakes"),
    ),
    kpiRow(
      "inaccuracies",
      "Inaccuracies",
      o.judgements.inaccuracies,
      "decimal",
      basisClass,
      weeklySeries(w, "inaccuracies"),
    ),
  ];
}

const scoreRows = (o: Overview): KpiRowView[] =>
  o.byTimeClass.map((tc) =>
    kpiRow(tc.timeClass, `${classLabel(tc.timeClass)} score`, tc.score, "rate", tc.timeClass),
  );

export type ViewB = {
  header: HeaderView;
  empty: EmptyState | null;
  lead: { accuracy: EstimateView; blunders: EstimateView };
  trend: TrendView;
  kpis: KpiRowView[];
  columns: JudgementColumnsGame[];
  results: ResultSplitRow[];
  scores: KpiRowView[];
};

/** Variant b: everything it renders, nothing more. With an empty state, no chart data at all. */
export function toViewB(o: Overview): ViewB {
  const empty = emptyState(o);
  const ready = empty === null;
  return {
    header: headerView(o),
    empty,
    lead: {
      accuracy: estimateView(o.accuracy.overall, "percent", "analysed"),
      blunders: estimateView(o.judgements.blunders, "decimal", "analysed"),
    },
    trend: ready ? trendView(o) : { points: [], rolling: [] },
    kpis: ready ? perGameRows(o, "analysed", "Accuracy") : [],
    columns: ready ? columnGames(o.judgements.columns) : [],
    results: ready ? splitRows(o.byTimeClass) : [],
    scores: ready ? scoreRows(o) : [],
  };
}

export type SpineRowView = {
  timeClass: TimeClass;
  name: string;
  games: number;
  split: ResultSplitRow;
  score: EstimateView;
  detail: {
    accuracy: KpiRowView;
    trend: TrendView;
    kpis: KpiRowView[];
    columns: JudgementColumnsGame[];
    columnsLabel: string;
  } | null;
};

export type ViewC = {
  header: HeaderView;
  empty: EmptyState | null;
  summary: { accuracy: EstimateView; blunders: EstimateView };
  /** The time control open on arrival: the one with the most analysed games. */
  open: TimeClass | null;
  rows: SpineRowView[];
};

/** Variant c: the spine rows, each with its own class's detail. */
export function toViewC(
  all: Overview,
  classes: { timeClass: TimeClass; overview: Overview }[],
): ViewC {
  const empty = emptyState(all);
  const rows: SpineRowView[] =
    empty !== null
      ? []
      : all.byTimeClass.map((tc) => {
          const name = classLabel(tc.timeClass);
          const o = classes.find((c) => c.timeClass === tc.timeClass)?.overview;
          const [split] = splitRows([tc]);
          return {
            timeClass: tc.timeClass,
            name,
            games: tc.games,
            split: { ...(split as ResultSplitRow), label: "Results" },
            score: estimateView(tc.score, "rate", tc.timeClass),
            detail: o
              ? (() => {
                  const [accuracy, ...kpis] = perGameRows(o, tc.timeClass, `${name} accuracy`);
                  return {
                    accuracy: accuracy as KpiRowView,
                    trend: trendView(o),
                    kpis: kpis.map((k) => ({ ...k, label: `${k.label} per game` })),
                    columns: columnGames(o.judgements.columns),
                    columnsLabel: `Your last ${o.judgements.columns.length} analysed ${name.toLowerCase()} games`,
                  };
                })()
              : null,
          };
        });
  const open = [...rows].sort((x, y) => y.games - x.games)[0]?.timeClass ?? null;
  return {
    header: headerView(all),
    empty,
    summary: {
      accuracy: estimateView(all.accuracy.overall, "percent", "analysed"),
      blunders: estimateView(all.judgements.blunders, "decimal", "analysed"),
    },
    open,
    rows,
  };
}
