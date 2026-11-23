// The profile copy helpers as plain functions, with no "use client", so a server component may call
// them (the band.ts lesson, M4 Task 8). Every helper guards its numbers: nothing here ever returns
// `NaN`, `Infinity`, `undefined` or `null` as text (DESIGN.md honesty rules, ADR 0072).

import { judgementGlyph, moveNumberText, type NotationJudgement } from "../board/notation.js";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY = 86_400_000;

/** Days since the epoch of an ISO date ("2026-11-09"), in UTC, so no time zone shifts a day. */
export function isoDay(iso: string): number {
  const [y = 0, m = 1, d = 1] = iso.slice(0, 10).split("-").map(Number);
  return Date.UTC(y, m - 1, d) / DAY;
}

/** "9 Nov 2026": UTC, day precision, British order, no locale lookups. */
export function dayText(iso: string): string {
  const date = new Date(isoDay(iso) * DAY);
  if (Number.isNaN(date.getTime())) return "an unknown date";
  return `${date.getUTCDate()} ${MONTHS[date.getUTCMonth()]} ${date.getUTCFullYear()}`;
}

/** "9 Nov", for axis ends where the year is stated elsewhere. */
export const shortDayText = (iso: string) => dayText(iso).replace(/ \d{4}$/, "");

/** "week of 9 Nov 2026" (lower case, for the middle of a sentence). */
export const weekText = (iso: string) => `week of ${dayText(iso)}`;

const fin = (n: number) => Number.isFinite(n);
const pct1 = (share: number) => (share * 100).toFixed(1);
const count = (n: number) => new Intl.NumberFormat("en-GB").format(n);

/**
 * "Estimated 4.2% of chances (80% range 3.1–5.6%), from 38 of 902". The stored posterior and its
 * stored 80% interval, one decimal each, exactly as given (never widened or recomputed), with the
 * raw counts beside them (ADR 0068 R3).
 */
export function weaknessRateText(
  r: { mean: number; low: number; high: number },
  e: { events: number; chances: number },
  /** The estimate's evidence window, as a phrase ("in the last 6 months"); omitted when unknown. */
  window?: string,
): string {
  const counts = `from ${count(e.events)} of ${count(e.chances)}${window ? ` ${window}` : ""}`;
  if (!fin(r.mean) || !fin(r.low) || !fin(r.high)) return `Rate not estimated yet, ${counts}`;
  return `Estimated ${pct1(r.mean)}% of chances (80% range ${pct1(r.low)}% to ${pct1(r.high)}%), ${counts}`;
}

/** "4.2%" for a share of chances. */
export const shareText = (share: number) => (fin(share) ? `${pct1(share)}%` : "not measured");

/** "3.1–5.6%". */
export const shareRangeText = (low: number, high: number) =>
  fin(low) && fin(high) ? `${pct1(low)}% to ${pct1(high)}%` : "range not measured";

/**
 * A weakness with no rating-band benchmark has no state worth naming: Rust stores `not_an_issue`
 * for it, which is no claim at all (bug hunt I1). These say so, never "not an issue".
 */
export const NOT_COMPARED_LABEL = "Not compared yet";
export const NO_BENCHMARK_TEXT = "Not compared yet: no benchmark for this account yet";
/**
 * Why a row the peer benchmark leaves out is not compared (holistic F1, ADR 0081): by design, a
 * measure mirrored or unenriched in the opponents' side of games against you.
 */
export const NOT_COMPARED_EXCLUDED_TEXT =
  "Not compared: this can't be measured fairly from your opponents' moves in games against you";
/** Why a peer row is not compared yet: it is stored low-sample. */
export const NOT_COMPARED_LOW_SAMPLE_TEXT = "Not compared yet: too few of their chances";

/** The reference group when the host does not name the band ("Players rated 1400–1600"). */
export const DEFAULT_BAND_LABEL = "Players in your rating band";

/**
 * How the stored interval sits against the rating band's median, naming the group: "Players
 * rated 1400–1600: 2.1%, lower than your whole likely range". Only an interval that clears
 * the median says lower or higher; one that contains it (ends included) says "inside", never
 * "about average" (no overclaiming). `basis` says what the figure is, in brackets after it
 * ("pooled over 65 of their chances").
 *
 * `confidence` is the stored `P(your rate > their rate)`. Inside the range at
 * [`PROBABLY_ABOVE`] or more it adds "though yours is probably higher (81% likely)": that is
 * the evidence an `active` state stands on (ADR 0084), so the row never reads as contradicting it.
 */
export function bandComparisonText(
  rate: { low: number; high: number },
  median: number | null,
  label: string = DEFAULT_BAND_LABEL,
  basis?: string,
  confidence?: number,
): string {
  if (median === null || !fin(median)) return NO_BENCHMARK_TEXT;
  const head = `${label}: ${shareText(median)}${basis ? ` (${basis})` : ""}`;
  if (median < rate.low) return `${head}, lower than your whole likely range`;
  if (median > rate.high) return `${head}, higher than your whole likely range`;
  if (probablyAbove(confidence)) {
    return `${head}, inside your likely range, though yours is probably higher (${likelyText(confidence)})`;
  }
  return `${head}, inside your likely range`;
}

/**
 * `P(rate > benchmark)` at or above which a weakness is probably worse than the benchmark: the
 * Rust timeline's `ACTIVE_CONFIDENCE`, the coach's focus threshold and its "probably" (ADR 0060).
 */
export const PROBABLY_ABOVE = 0.7;

/** Whether a stored confidence is a probability at or above [`PROBABLY_ABOVE`]. */
export const probablyAbove = (c: number | null | undefined): c is number =>
  typeof c === "number" && fin(c) && c >= PROBABLY_ABOVE && c <= 1;

/** "81% likely" (whole percent, never 100% when the store said less). */
export const likelyText = (c: number) => `${Math.min(99, Math.round(c * 100))}% likely`;

/** Plural nouns for what a weakness counts: "38 errors in 902 rook-endgame moves". */
export interface WeaknessCounts {
  event: string;
  chance: string;
}
export const DEFAULT_COUNTS: WeaknessCounts = { event: "times", chance: "chances" };

/** "38 times in 902 chances". */
export const countsText = (
  e: { events: number; chances: number },
  c: WeaknessCounts = DEFAULT_COUNTS,
) => `${count(e.events)} ${c.event} in ${count(e.chances)} ${c.chance}`;

/**
 * One interval scale for a list of rows, from zero to a little past the largest high end or band
 * median, rounded up to a whole percent (to 5% above 10%), so rows compare down the column. A rate
 * is a share of chances, so the scale never runs past 100%.
 */
export function weaknessScaleMax(
  rows: { rate: { high: number }; band: { median: number | null } }[],
): number {
  const top = Math.max(
    0,
    ...rows.flatMap((r) => [r.rate.high, r.band.median ?? 0]).filter((v) => fin(v)),
  );
  if (top <= 0) return 0.01;
  const step = top * 1.1 > 0.1 ? 0.05 : 0.01;
  return Math.min(1, Math.ceil((top * 1.1) / step - 1e-9) * step);
}

/** "Weekly trend after 180 more chances", or a plain line when the shortfall is unknown. */
export const trendPendingText = (needMore: number | null, c: WeaknessCounts = DEFAULT_COUNTS) =>
  needMore === null || !fin(needMore) || needMore <= 0
    ? "Weekly trend after more weeks"
    : `Weekly trend after ${count(needMore)} more ${needMore === 1 ? singular(c.chance) : c.chance}`;

/** "chances" to "chance", "rook-endgame moves" to "rook-endgame move". */
const singular = (plural: string) => plural.replace(/s$/, "");

/** "180 more chances needed" (one is singular), or a plain line when the shortfall is not known. */
export const needMoreText = (needMore: number | null, c: WeaknessCounts = DEFAULT_COUNTS) =>
  needMore === null || !fin(needMore) || needMore <= 0
    ? `More ${c.chance} needed`
    : `${count(needMore)} more ${needMore === 1 ? singular(c.chance) : c.chance} needed`;

// ---- KPI trends ----

/** The catalogue's units (`profile_core::kpi`): `rate` is 0 to 1, `pct` 0 to 100. */
export type KpiUnit = "rate" | "per_game" | "pct" | "seconds" | "elo";

/**
 * A KPI value at its unit's precision: `rate` and `pct` as a percent to 0.1, `per_game` to 0.01,
 * `seconds` and `elo` whole. A non-finite value reads "not measured", never `NaN`.
 */
export function formatKpiValue(value: number, unit: string | null): string {
  if (!fin(value)) return "not measured";
  switch (unit) {
    case "rate":
      return `${pct1(value)}%`;
    case "pct":
      return `${value.toFixed(1)}%`;
    case "seconds":
      return `${Math.round(value)} s`;
    case "elo":
      return count(Math.round(value));
    case "per_game":
    case null:
      return value.toFixed(2);
    default:
      return `${value.toFixed(2)} ${unit}`;
  }
}

export interface KpiTrendPoint {
  /** The ISO week's Monday ("2026-11-09"). */
  week: string;
  value: number | null;
  /** The stored band's ends, drawn exactly as given; null draws no band for that week. */
  low: number | null;
  high: number | null;
  /** Games (or chances, see `nUnit`) behind the value. */
  n: number;
}

export interface KpiTrendItem {
  metric: string;
  label: string;
  unit: string | null;
  points: KpiTrendPoint[];
  /** What `n` counts, singular and plural (default games). */
  nUnit?: { one: string; many: string };
  /** The glossary entry: its meaning under the heading, and an "i" that opens the rest. */
  explain?: {
    name: string;
    meaning: string;
    measured: string;
    whyItMatters: string;
    tip: string;
  } | null;
}

const GAMES = { one: "game", many: "games" };

/** "12 games", "1 game". */
export const nText = (n: number, unit: KpiTrendItem["nUnit"] = GAMES) =>
  `${count(n)} ${n === 1 ? unit.one : unit.many}`;

/** Whether a point is drawn on the line: a value, finite, with at least `minN` behind it. */
export const onLine = (p: KpiTrendPoint, minN: number) =>
  p.value !== null && fin(p.value) && p.n >= minN;

/** One week in words: its value and n, or why it has none. */
export function kpiPointText(item: KpiTrendItem, p: KpiTrendPoint, minN: number): string {
  const unit = item.nUnit ?? GAMES;
  const week = weekText(p.week);
  if (p.value === null || !fin(p.value)) {
    return p.n === 0
      ? `no ${unit.many} in ${week}`
      : `not measured in ${week}, ${nText(p.n, unit)}`;
  }
  const value = `${formatKpiValue(p.value, item.unit)} in ${week}, ${nText(p.n, unit)}`;
  return p.n < minN ? `${value}, below the ${count(minN)}-${unit.one} minimum` : value;
}

/**
 * The accessible summary of one small multiple: "Blunders per game: 1.20 in week of 2 Nov 2026,
 * 12 games; no games in week of 9 Nov 2026; …". Every week is named, gaps included.
 */
export function kpiSummary(item: KpiTrendItem, minN: number): string {
  if (item.points.length === 0) return `${item.label}: no weeks measured yet.`;
  return `${item.label}: ${item.points.map((p) => kpiPointText(item, p, minN)).join("; ")}.`;
}

// ---- Exemplar caption ----

export interface ExemplarCaption {
  /** The stored judgement of the played move; the UI never reclassifies it. */
  judgement?: NotationJudgement | null;
  /** The mover's win chance before and after the move, 0 to 100. */
  winBefore: number | null;
  winAfter: number | null;
}

/** The move number and colour to move, from the FEN before the move. */
export function fenMove(fen: string): { moveNumber: number; color: "white" | "black" } {
  const [, side = "w", , , , full = "1"] = fen.trim().split(/\s+/);
  const n = Number.parseInt(full, 10);
  return { moveNumber: fin(n) && n > 0 ? n : 1, color: side === "b" ? "black" : "white" };
}

/** "4. Nxe5" or "4… Nf6": the played move's notation, without its glyph. */
export function exemplarMoveText(fen: string, san: string): string {
  const m = fenMove(fen);
  return `${moveNumberText(m.moveNumber, m.color)} ${san}`;
}

/** "4. Nxe5? Better was Nxd4. Win chance 52% to 36%.": each sentence only when its data exists. */
export function exemplarCaption(p: {
  fen: string;
  played: { san: string; uci?: string };
  best: { san: string; uci?: string } | null;
  refutation?: string | null;
  caption: ExemplarCaption;
}): string {
  const glyph = p.caption.judgement ? judgementGlyph[p.caption.judgement] : "";
  // A glyph ends the sentence itself ("14. d4? Better was…"); a bare move takes a full stop.
  const parts = [`${exemplarMoveText(p.fen, p.played.san)}${glyph || "."}`];
  if (p.best && p.best.san !== p.played.san) parts.push(`Better was ${p.best.san}.`);
  const { winBefore: b, winAfter: a } = p.caption;
  if (b !== null && a !== null && fin(b) && fin(a)) {
    const pc = (v: number) => Math.round(Math.min(100, Math.max(0, v)));
    parts.push(`Win chance ${pc(b)}% to ${pc(a)}%.`);
  }
  return parts.join(" ");
}
