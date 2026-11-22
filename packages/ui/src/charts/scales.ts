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
  // Month labels only on month starts, and one label per day: a span of a day or two (one game)
  // ticks by the hour, and two labels must never read the same.
  const seen = new Set<string>();
  const midnights = raw.filter((d) => d.getTime() % DAY === 0);
  const daily = midnights.length > 0 ? midnights : raw;
  const ticks = (byMonth ? raw.filter((d) => d.getUTCDate() === 1) : daily).filter((d) => {
    const day = d.toISOString().slice(0, 10);
    if (seen.has(day)) return false;
    seen.add(day);
    return true;
  });
  return ticks.map((d) => ({
    x: scale(d),
    iso: d.toISOString().slice(0, 10),
    text: byMonth
      ? `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`
      : `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`,
  }));
}

/** Contiguous runs of `items` where `has` holds, so a gap breaks a line instead of bridging it. */
export function runs<T>(
  items: T[],
  has: (item: T) => boolean,
  /** False between two neighbours that must not be joined (a span with no games). */
  joins: (prev: T, next: T) => boolean = () => true,
): T[][] {
  const out: T[][] = [];
  let current: T[] = [];
  for (const item of items) {
    const prev = current[current.length - 1];
    if (prev !== undefined && has(item) && !joins(prev, item)) {
      out.push(current);
      current = [];
    }
    if (has(item)) current.push(item);
    else if (current.length) {
      out.push(current);
      current = [];
    }
  }
  if (current.length) out.push(current);
  return out;
}

/** How far a tap or click reaches for its nearest dot, in px. */
export const HIT_RADIUS = { fine: 12, coarse: 22 } as const;

/**
 * Over half a game per px of plot, dots stay at the fine radius and step one tone down, so the
 * mean line stays the strongest mark (the nearest-dot overlay keeps them reachable by touch).
 */
export function isDense(count: number, width: number): boolean {
  const plot = Math.max(1, width - TREND_LEFT - TREND_RIGHT);
  return count / plot > 0.5;
}

/**
 * The index of the dot nearest (x, y) within `radius`, or null. `dots` are in x order, so a
 * binary search finds the window and only dots within `radius` of x are measured.
 */
export function nearestIndex(
  dots: readonly { x: number; y: number }[],
  x: number,
  y: number,
  radius: number,
): number | null {
  let lo = 0;
  let hi = dots.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((dots[mid]?.x ?? 0) < x - radius) lo = mid + 1;
    else hi = mid;
  }
  let best: number | null = null;
  let bestD = radius * radius;
  for (let i = lo; i < dots.length; i++) {
    const d = dots[i] as { x: number; y: number };
    if (d.x > x + radius) break;
    const dist = (d.x - x) ** 2 + (d.y - y) ** 2;
    if (dist <= bestD) {
      bestD = dist;
      best = i;
    }
  }
  return best;
}

/** "12 Nov 2026", in UTC (ruling R6). */
export function dayText(date: Date): string {
  return `${date.getUTCDate()} ${MONTHS[date.getUTCMonth()]} ${date.getUTCFullYear()}`;
}

/**
 * The trend in words for screen readers: the games and their span, then the latest rolling
 * average with its 80% band, ends rounded outward (DESIGN.md "KPI stats").
 */
export function trendSummary(
  games: readonly { id: string; time: Date }[],
  rolling: readonly { id: string; mean: number; low: number | null; high: number | null }[],
): string {
  const first = games[0];
  const last = games[games.length - 1];
  if (!first || !last) return "";
  const n = games.length;
  let text = `${n} ${n === 1 ? "game" : "games"}, ${dayText(first.time)} to ${dayText(last.time)}.`;
  const byId = new Map(rolling.map((r) => [r.id, r]));
  for (let i = games.length - 1; i >= 0; i--) {
    const g = games[i] as { id: string; time: Date };
    const r = byId.get(g.id);
    if (!r || !Number.isFinite(r.mean)) continue;
    text += ` Rolling average ${r.mean.toFixed(1)}% on ${dayText(g.time)}`;
    const { low, high } = r;
    if (
      low !== null &&
      high !== null &&
      Number.isFinite(low) &&
      Number.isFinite(high) &&
      high > low
    ) {
      const lo = (Math.floor(low * 10) / 10).toFixed(1);
      const hi = (Math.ceil(high * 10) / 10).toFixed(1);
      text += `, 80% likely between ${lo}% and ${hi}%`;
    }
    text += ".";
    break;
  }
  return text;
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

/** A count as whole games or moves: a negative, fractional or non-finite count never draws. */
export const whole = (n: number) => (Number.isFinite(n) && n > 0 ? Math.floor(n) : 0);

/** "2 blunders, 1 mistake, 3 inaccuracies", or "no marked moves". */
export function countText(c: { blunders: number; mistakes: number; inaccuracies: number }): string {
  if (c.blunders + c.mistakes + c.inaccuracies === 0) return "no marked moves";
  return [
    plural(c.blunders, "blunder", "blunders"),
    plural(c.mistakes, "mistake", "mistakes"),
    plural(c.inaccuracies, "inaccuracy", "inaccuracies"),
  ].join(", ");
}

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
