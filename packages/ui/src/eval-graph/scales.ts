// Pure geometry for the eval graph, kept apart so it can be tested without rendering.
import { Scale } from "@visx/visx";

// Layout, from DESIGN.md "Eval graph": a 22px label lane above the plot and a phase lane below.
export const LANE = 22;
export const AXIS = 18;
export const PAD = 8;
/**
 * Below this container width the plot is 88px, labels drop the move number and the readout takes
 * the phase lane. It mirrors `--container-graph` in tokens.css (the `@graph` container query),
 * which a test keeps in step.
 */
export const GRAPH_NARROW = 496;
/** From this container width, inaccuracies are labelled too. */
export const LABEL_WIDE = 768;
export const PLOT_HEIGHT = { narrow: 88, wide: 120 } as const;

/** x of the `ply`-th point (1-based) across `width`: the first at 0 and the last at `width`. */
export function xForPly(ply: number, width: number, count: number): number {
  if (count <= 1) return width / 2;
  return Scale.scaleLinear<number>({ domain: [1, count], range: [0, width], clamp: true })(ply);
}

/** y of White's win percent (0 to 100) in a plot `height` tall, White winning at the top. */
export function yForWin(win: number, height: number): number {
  return Scale.scaleLinear<number>({ domain: [0, 100], range: [height, 0], clamp: true })(win);
}

/** The 1-based point index nearest to `x`, clamped to 1..count. */
export function plyForX(x: number, width: number, count: number): number {
  if (count <= 1 || width <= 0) return 1;
  const ply = Math.round((x / width) * (count - 1)) + 1;
  return Math.min(count, Math.max(1, ply));
}

// --- Label lane -------------------------------------------------------------------------------

export type Judgement = "inaccuracy" | "mistake" | "blunder";
export type LabelForm = "full" | "san";

export interface LabelMark {
  ply: number;
  san: string;
  judgement: Judgement;
  /** x of the mark's tick. */
  x: number;
  /** y of the top of the mark's tick. */
  top: number;
  /** True for the player's own move, false for the opponent's, null when the player is unknown. */
  mine: boolean | null;
}

export interface LabelText {
  prefix: string;
  san: string;
  glyph: string;
}

export interface PlacedLabel {
  ply: number;
  x: number;
  width: number;
  tickX: number;
  tickTop: number;
  serious: boolean;
  /** Always true since M5a T10 (no glyph-only labels); kept for the leader layer. */
  leader: boolean;
  /** Whose move: the player's (true), the opponent's (false) or unknown (null). */
  mine: boolean | null;
  text: LabelText;
  /** The move's judgement, for its severity colour. */
  judgement?: Judgement;
}

export const GLYPH: Record<Judgement, string> = { blunder: "??", mistake: "?", inaccuracy: "?!" };

const LABEL_GAP = 10;
const MAX_PUSH = 56;
const SERIOUS_CLEARANCE = 24;
const INACCURACY_SPACING = 120;
/** Leaders leave the label at this y, bend at LEADER_BEND, then run straight down. */
export const LEADER_TOP = 18;
export const LEADER_BEND = LEADER_TOP + 7;

/** Literata at 14px averages under 0.6em a character; placement only needs a safe estimate. */
export const labelWidth = (t: LabelText) => Math.ceil((t.prefix + t.san + t.glyph).length * 8.4);

/** Move number and side prefix: `16. ` for White, `9…` for Black. */
export const movePrefix = (ply: number) =>
  ply % 2 === 1 ? `${Math.ceil(ply / 2)}. ` : `${Math.ceil(ply / 2)}…`;

const textFor = (m: LabelMark, form: LabelForm): LabelText => ({
  prefix: form === "full" ? movePrefix(m.ply) : "",
  san: m.san,
  glyph: GLYPH[m.judgement],
});

const isSerious = (m: LabelMark) => m.judgement !== "inaccuracy";
const mineRank = (m: LabelMark) => (m.mine === false ? 1 : 0);
const severityRank = (m: LabelMark) => (m.judgement === "blunder" ? 0 : 1);
/** A tick this close to the lane gets a straight vertical leader, so its label must cover it. */
const nearTop = (m: LabelMark) => m.top - 2 <= LEADER_BEND;

/**
 * Whether a label for `m` may span [a, b]: inside the lane, clear of every placed label, and in
 * the same left-to-right order as its tick, so no two leaders ever cross.
 */
function fits(m: LabelMark, a: number, b: number, width: number, placed: PlacedLabel[]): boolean {
  if (a < 0 || b > width) return false;
  if (nearTop(m) && (m.x < a + 3 || m.x > b - 3)) return false;
  return placed.every((t) => {
    const gap = t.serious && !isSerious(m) ? SERIOUS_CLEARANCE : LABEL_GAP;
    const before = m.x < t.tickX || (m.x === t.tickX && m.ply < t.ply);
    return before ? b + gap <= t.x : a >= t.x + t.width + gap;
  });
}

function placeSerious(marks: LabelMark[], width: number, form: LabelForm): PlacedLabel[] {
  const placed: PlacedLabel[] = [];
  const place = (m: LabelMark, text: LabelText, starts: number[], leader: boolean) => {
    const w = labelWidth(text);
    const start = starts.find((s) => fits(m, s, s + w, width, placed));
    if (start === undefined) return false;
    placed.push({
      ply: m.ply,
      x: start,
      width: w,
      tickX: m.x,
      tickTop: m.top,
      serious: true,
      leader,
      mine: m.mine,
      text,
      judgement: m.judgement,
    });
    return true;
  };

  for (const m of marks) {
    const text = textFor(m, form);
    const base = Math.min(Math.max(m.x - 6, 0), width - labelWidth(text));
    const pushes = [base];
    for (let d = 1; d <= MAX_PUSH; d++) pushes.push(base + d, base - d);
    // A label that can't be placed is dropped: a bare glyph with no move read as noise (M5a T10).
    place(m, text, pushes, true);
  }
  return placed;
}

/**
 * Places move labels in the lane above the plot. Mistakes and blunders go first, in priority
 * order (the player's blunders, the player's mistakes, then the opponent's), each pushed either
 * way by at most 56px to keep a 10px gap, and always in the same order as their ticks, so leaders
 * never cross. One label form is used for the whole lane: the full notation (`28. Nd2??`) when it
 * places as many labels as SAN alone, else SAN plus glyph (always when `narrow`). A label that
 * can't be placed is dropped (never left as a bare glyph); the text alternative still lists it.
 * `sparse` (a long game) labels blunders only and leaves mistakes as ticks. On `wide` lanes that
 * aren't sparse, inaccuracies are labelled where they fit unpushed, 24px clear of serious labels
 * and at most one per 120px.
 */
export function placeLabels(
  marks: LabelMark[],
  width: number,
  { narrow, wide, sparse = false }: { narrow: boolean; wide: boolean; sparse?: boolean },
): PlacedLabel[] {
  const serious = marks
    .filter((m) => (sparse ? m.judgement === "blunder" : isSerious(m)))
    .sort((a, b) => mineRank(a) - mineRank(b) || severityRank(a) - severityRank(b) || a.x - b.x);
  const withLeaders = (ls: PlacedLabel[]) => ls.filter((l) => l.leader).length;
  let form: LabelForm = "san";
  let placed = placeSerious(serious, width, "san");
  if (!narrow) {
    const full = placeSerious(serious, width, "full");
    if (withLeaders(full) >= withLeaders(placed)) {
      form = "full";
      placed = full;
    }
  }
  if (!wide || sparse) return placed;

  const inaccuracies = marks
    .filter((m) => !isSerious(m))
    .sort((a, b) => mineRank(a) - mineRank(b) || a.x - b.x);
  const all = [...placed];
  for (const m of inaccuracies) {
    const text = textFor(m, form);
    const w = labelWidth(text);
    const x = Math.min(Math.max(m.x - 6, 0), width - w);
    if (!fits(m, x, x + w, width, all)) continue;
    if (all.some((t) => !t.serious && Math.abs(t.x - x) < INACCURACY_SPACING)) continue;
    all.push({
      ply: m.ply,
      x,
      width: w,
      tickX: m.x,
      tickTop: m.top,
      serious: false,
      leader: true,
      mine: m.mine,
      text,
      judgement: m.judgement,
    });
  }
  return all;
}

export type Point = readonly [number, number];

/**
 * A label's leader: a 7px diagonal under the label, then straight down to just above its tick.
 * A tick at the top of the plot gets a straight vertical from the label instead.
 */
export function leaderPoints(l: PlacedLabel): Point[] {
  const anchor = Math.min(Math.max(l.tickX, l.x + 3), l.x + l.width - 3);
  const end = Math.max(LANE, l.tickTop - 2);
  if (end <= LEADER_BEND) {
    return anchor === l.tickX
      ? [
          [l.tickX, LEADER_TOP],
          [l.tickX, end],
        ]
      : [
          [anchor, LEADER_TOP],
          [l.tickX, end],
        ];
  }
  return [
    [anchor, LEADER_TOP],
    [l.tickX, LEADER_BEND],
    [l.tickX, end],
  ];
}

// --- Corner labels ----------------------------------------------------------------------------

export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Schibsted at 12px averages about 0.55em a character. */
export const cornerWidth = (text: string) => Math.ceil(text.length * 6.6);

/** Liang-Barsky: does the segment from `a` to `b` touch the box? */
export function segmentHitsBox(a: Point, b: Point, box: Box): boolean {
  let t0 = 0;
  let t1 = 1;
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const edges: [number, number][] = [
    [-dx, a[0] - box.x0],
    [dx, box.x1 - a[0]],
    [-dy, a[1] - box.y0],
    [dy, box.y1 - a[1]],
  ];
  for (const [p, q] of edges) {
    if (p === 0) {
      if (q < 0) return false;
      continue;
    }
    const r = q / p;
    if (p < 0) t0 = Math.max(t0, r);
    else t1 = Math.min(t1, r);
    if (t0 > t1) return false;
  }
  return true;
}

export type Side = "left" | "right";

/** Clearance around a corner label's ink: the eval line's 2px stroke plus its halo. */
const CORNER_CLEAR = 3;

/** The box a corner label's ink occupies inside the plot, grown by CORNER_CLEAR on every side. */
export function cornerBox(
  corner: "top" | "bottom",
  side: Side,
  textWidth: number,
  width: number,
  plotH: number,
): Box {
  // 12px Schibsted, baseline at LANE + 14 (top) or LANE + plotH - 6 (bottom): cap top 9px above
  // the baseline, descender 3px below.
  const baseline = corner === "top" ? LANE + 14 : LANE + plotH - 6;
  const x0 = side === "left" ? PAD : width - PAD - textWidth;
  return {
    x0: x0 - CORNER_CLEAR,
    x1: x0 + textWidth + CORNER_CLEAR,
    y0: baseline - 9 - CORNER_CLEAR,
    y1: baseline + 3 + CORNER_CLEAR,
  };
}

export interface CornerObstacles {
  labels: PlacedLabel[];
  ticks: { x: number; y1: number; y2: number }[];
  /** The eval line's vertices. */
  line: Point[];
}

/** Every leader, tick and line segment that touches `box`. */
export function collisions(box: Box, { labels, ticks, line }: CornerObstacles): number {
  const segments: [Point, Point][] = [];
  for (let i = 1; i < line.length; i++) segments.push([line[i - 1] as Point, line[i] as Point]);
  for (const l of labels.filter((l) => l.leader)) {
    const pts = leaderPoints(l);
    for (let i = 1; i < pts.length; i++) segments.push([pts[i - 1] as Point, pts[i] as Point]);
  }
  for (const t of ticks) {
    segments.push([
      [t.x, t.y1],
      [t.x, t.y2],
    ]);
  }
  return segments.filter(([a, b]) => segmentHitsBox(a, b, box)).length;
}

/**
 * Puts each corner label ("White (you)", "Black") at the left end of the plot, and moves it to
 * the right end only when nothing at all (leader, tick or line) touches it there. Otherwise it
 * stays left and its halo breaks whatever crosses it.
 */
export function placeCorners({
  width,
  plotH,
  top,
  bottom,
  measure = cornerWidth,
  ...obstacles
}: CornerObstacles & {
  width: number;
  plotH: number;
  top: string;
  bottom: string;
  /** The text's rendered width; defaults to an estimate. */
  measure?: (text: string) => number;
}): { top: Side; bottom: Side } {
  const side = (corner: "top" | "bottom", text: string): Side => {
    const w = measure(text);
    if (collisions(cornerBox(corner, "left", w, width, plotH), obstacles) === 0) return "left";
    return collisions(cornerBox(corner, "right", w, width, plotH), obstacles) === 0
      ? "right"
      : "left";
  };
  return { top: side("top", top), bottom: side("bottom", bottom) };
}

// --- Readout ----------------------------------------------------------------------------------

export const READOUT_W = 148;
export const READOUT_H = 48;

/**
 * Where the hover popover goes, from the threshold up: beside the hovered x on the side away from
 * the cursor rule (and never over it), clear of the corner label in its band, in the half of the
 * plot away from the line unless the other half covers fewer points.
 */
export function readoutPlacement({
  x,
  lineY,
  cursorX,
  xs,
  ys,
  plotH,
  width,
  corners,
}: {
  x: number;
  lineY: number;
  cursorX?: number;
  xs: number[];
  ys: number[];
  plotH: number;
  width: number;
  corners: { top: { side: Side; width: number }; bottom: { side: Side; width: number } };
}): { left: number; top: number } {
  const rightOf = x + 12;
  const leftOf = x - 12 - READOUT_W;
  const preferLeft = cursorX !== undefined && cursorX > x ? true : rightOf + READOUT_W > width;
  const covers = (l: number) =>
    cursorX !== undefined && cursorX >= l - 6 && cursorX <= l + READOUT_W + 6;
  const leftFor = (corner: { side: Side; width: number }) => {
    const reserved = PAD + corner.width + 8;
    const lo = corner.side === "left" ? reserved : 0;
    const hi = width - READOUT_W - (corner.side === "right" ? reserved : 0);
    const clamp = (l: number) => Math.min(Math.max(l, lo), Math.max(lo, hi));
    const first = clamp(preferLeft ? leftOf : rightOf);
    if (!covers(first)) return first;
    const second = clamp(preferLeft ? rightOf : leftOf);
    return covers(second) ? first : second;
  };
  const bands = {
    top: { top: LANE + 6, left: leftFor(corners.top) },
    bottom: { top: LANE + plotH - 6 - READOUT_H, left: leftFor(corners.bottom) },
  };
  const covered = ({ top, left }: { top: number; left: number }) =>
    xs.reduce((n, px, i) => {
      const py = ys[i] ?? 0;
      const inX = px >= left - 4 && px <= left + READOUT_W + 4;
      return inX && py + 9 >= top && py - 9 <= top + READOUT_H ? n + 1 : n;
    }, 0);
  const preferred = lineY < LANE + plotH / 2 ? bands.bottom : bands.top;
  const other = preferred === bands.top ? bands.bottom : bands.top;
  return covered(preferred) <= covered(other) ? preferred : other;
}
