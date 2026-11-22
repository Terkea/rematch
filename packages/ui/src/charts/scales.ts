// Pure geometry and copy for the overview charts, kept apart so they can be tested without
// rendering (ADR 0050).
import { Scale } from "@visx/visx";

// --- Accuracy trend ---------------------------------------------------------------------------

/** The whole trend box is this tall at every width, so nothing shifts when it measures itself. */
export const TREND_HEIGHT = 208;
/** Room above the plot for the top dot and its halo. */
export const TREND_TOP = 10;
/** The date axis lane under the plot. */
export const TREND_AXIS = 26;
/** The accuracy labels' column, left of the plot. */
export const TREND_LEFT = 30;
/** Room right of the plot for the last dot and its halo. */
export const TREND_RIGHT = 8;
export const TREND_PLOT_H = TREND_HEIGHT - TREND_TOP - TREND_AXIS;
/** Accuracy gridlines. 0 and 100 are the plot's own edges. */
export const ACCURACY_TICKS = [25, 50, 75] as const;
/** Dot radius in px: R11 shapes, larger on a coarse pointer. */
export const DOT_R = { fine: 3, coarse: 4.5 } as const;

const DAY = 86_400_000;

/** A UTC time scale over the points' dates. One date (or one day) is widened to a day each way. */
export function timeScale(dates: Date[], width: number) {
  const ms = dates.map((d) => d.getTime());
  let lo = Math.min(...ms);
  let hi = Math.max(...ms);
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) {
    lo = 0;
    hi = DAY;
  }
  if (hi - lo < DAY) {
    lo -= DAY;
    hi += DAY;
  }
  return Scale.scaleUtc<number>({
    domain: [new Date(lo), new Date(hi)],
    range: [TREND_LEFT, Math.max(TREND_LEFT, width - TREND_RIGHT)],
    clamp: true,
  });
}

/** y of an accuracy (0 to 100) in the plot, 100 at the top. */
export function yForAccuracy(accuracy: number): number {
  return Scale.scaleLinear<number>({
    domain: [0, 100],
    range: [TREND_TOP + TREND_PLOT_H, TREND_TOP],
    clamp: true,
  })(accuracy);
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * Date axis labels, in UTC: "12 Nov" across a few months or less, "Nov 2026" across longer
 * spans, so a label never claims more precision than its spacing.
 */
export function dateTicks(scale: ReturnType<typeof timeScale>, width: number) {
  const [from, to] = scale.domain() as [Date, Date];
  const span = to.getTime() - from.getTime();
  const count = Math.max(2, Math.floor((width - TREND_LEFT) / 96));
  const byMonth = span > 92 * DAY;
  const raw = scale.ticks(count);
  // Month labels only on month starts, so two labels never read the same.
  const ticks = byMonth ? raw.filter((d) => d.getUTCDate() === 1) : raw;
  return ticks.map((d) => ({
    x: scale(d),
    iso: d.toISOString().slice(0, 10),
    text: byMonth
      ? `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`
      : `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`,
  }));
}

/** Contiguous runs of `items` where `has` holds, so a gap breaks a line instead of bridging it. */
export function runs<T>(items: T[], has: (item: T) => boolean): T[][] {
  const out: T[][] = [];
  let current: T[] = [];
  for (const item of items) {
    if (has(item)) current.push(item);
    else if (current.length) {
      out.push(current);
      current = [];
    }
  }
  if (current.length) out.push(current);
  return out;
}

// --- Judgement columns ------------------------------------------------------------------------

/** Below this container width the columns keep their least width and scroll (480 / 40 games). */
export const COLUMNS_MIN_WIDTH = 480;
export const COLUMN_MIN_W = 12;
/** The gap between columns and between stacked cells: the surface shows through (dataviz). */
export const CELL_GAP = 2;
/** One move's cell height, until a tall column needs it smaller to fit COLUMNS_MAX_H. */
export const CELL_H = 6;
export const COLUMNS_MAX_H = 132;
/** The least plot height, so a run of clean games still reads as a chart. */
export const COLUMNS_MIN_ROWS = 6;

/** The plot's px width: the container's, or the columns' least width when that is wider. */
export function columnsWidth(container: number, count: number): number {
  return Math.max(container, count * COLUMN_MIN_W);
}

/** Cell height for the tallest column, so every column fits COLUMNS_MAX_H. Never below 2px. */
export function cellHeight(maxCells: number): number {
  const rows = Math.max(maxCells, COLUMNS_MIN_ROWS);
  const fit = (COLUMNS_MAX_H - rows * CELL_GAP) / rows;
  return Math.max(2, Math.min(CELL_H, fit));
}

// --- Copy -------------------------------------------------------------------------------------

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "8 wins, 2 draws, 4 losses", with singular forms. */
export function resultText(wins: number, draws: number, losses: number): string {
  return [
    plural(wins, "win", "wins"),
    plural(draws, "draw", "draws"),
    plural(losses, "loss", "losses"),
  ].join(", ");
}

/** A segment's share of its row in percent, to two places. */
export const share = (part: number, total: number) =>
  total > 0 ? Math.round((part / total) * 10_000) / 100 : 0;
