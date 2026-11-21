// Pure geometry for the eval graph, kept apart so it can be tested without rendering.
import { Scale } from "@visx/visx";

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

type Bezier = readonly [number, number, number, number];

const cubic = (a: number, b: number, s: number) =>
  3 * (1 - s) * (1 - s) * s * a + 3 * (1 - s) * s * s * b + s * s * s;

/**
 * When a wipe eased by `bezier` over `duration` reaches `progress` (0 to 1) of its width, in the
 * same unit as `duration`. Marks fade in as the reveal passes them.
 */
export function wipeDelay(progress: number, duration: number, bezier: Bezier): number {
  const p = Math.min(1, Math.max(0, progress));
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 32; i++) {
    const mid = (lo + hi) / 2;
    if (cubic(bezier[1], bezier[3], mid) < p) lo = mid;
    else hi = mid;
  }
  return cubic(bezier[0], bezier[2], (lo + hi) / 2) * duration;
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
  serious: boolean;
  /** False for a glyph-only label sitting straight above its tick. */
  leader: boolean;
  text: LabelText;
}

export const GLYPH: Record<Judgement, string> = { blunder: "??", mistake: "?", inaccuracy: "?!" };

const LABEL_GAP = 10;
const MAX_PUSH = 56;
const GLYPH_SLOP = 4;
const SERIOUS_CLEARANCE = 24;
const INACCURACY_SPACING = 120;

/** Literata at 14px averages under 0.6em a character; placement only needs a safe estimate. */
export const labelWidth = (t: LabelText) => Math.ceil((t.prefix + t.san + t.glyph).length * 8.4);

/** Move number and side prefix: `16. ` for White, `9…` for Black. */
export const movePrefix = (ply: number) =>
  ply % 2 === 1 ? `${Math.ceil(ply / 2)}. ` : `${Math.ceil(ply / 2)}…`;

const textFor = (m: LabelMark, form: LabelForm | "glyph"): LabelText => ({
  prefix: form === "full" ? movePrefix(m.ply) : "",
  san: form === "glyph" ? "" : m.san,
  glyph: GLYPH[m.judgement],
});

const isSerious = (m: LabelMark) => m.judgement !== "inaccuracy";
const mineRank = (m: LabelMark) => (m.mine === false ? 1 : 0);
const severityRank = (m: LabelMark) => (m.judgement === "blunder" ? 0 : 1);

function placeSerious(marks: LabelMark[], width: number, form: LabelForm): PlacedLabel[] {
  const placed: PlacedLabel[] = [];
  const fits = (a: number, b: number) =>
    a >= 0 &&
    b <= width &&
    placed.every((t) => b + LABEL_GAP <= t.x || a >= t.x + t.width + LABEL_GAP);

  for (const m of marks) {
    const text = textFor(m, form);
    const w = labelWidth(text);
    const base = Math.min(Math.max(m.x - 6, 0), width - w);
    let start: number | undefined;
    for (let d = 0; d <= MAX_PUSH && start === undefined; d += 1) {
      for (const s of d === 0 ? [base] : [base + d, base - d]) {
        if (fits(s, s + w)) {
          start = s;
          break;
        }
      }
    }
    if (start !== undefined) {
      placed.push({
        ply: m.ply,
        x: start,
        width: w,
        tickX: m.x,
        serious: true,
        leader: true,
        text,
      });
      continue;
    }
    // Glyph only, and only straight above its tick (no leader); otherwise the label is dropped.
    const glyph = textFor(m, "glyph");
    const gw = labelWidth(glyph);
    for (let d = 0; d <= GLYPH_SLOP && start === undefined; d += 1) {
      for (const s of d === 0 ? [m.x - gw / 2] : [m.x - gw / 2 + d, m.x - gw / 2 - d]) {
        if (fits(s, s + gw)) {
          start = s;
          break;
        }
      }
    }
    if (start !== undefined) {
      placed.push({
        ply: m.ply,
        x: start,
        width: gw,
        tickX: m.x,
        serious: true,
        leader: false,
        text: glyph,
      });
    }
  }
  return placed;
}

/**
 * Places move labels in the lane above the plot. Mistakes and blunders go first, in priority
 * order (the player's blunders, the player's mistakes, then the opponent's), each pushed left or
 * right by at most 56px to keep a 10px gap. One label form is used for the whole lane: the full
 * notation (`28. Nd2??`) when it places as many labels as SAN alone, else SAN plus glyph (always
 * below `narrow`). A label that can't be placed falls back to its glyph straight above the tick
 * (within 4px, no leader), or is dropped; the text alternative still lists it. On `wide` lanes,
 * inaccuracies are labelled where they fit unpushed, 24px clear of serious labels and at most
 * one per 120px.
 */
export function placeLabels(
  marks: LabelMark[],
  width: number,
  { narrow, wide }: { narrow: boolean; wide: boolean },
): PlacedLabel[] {
  const serious = marks
    .filter(isSerious)
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
  if (!wide) return placed;

  const inaccuracies = marks
    .filter((m) => !isSerious(m))
    .sort((a, b) => mineRank(a) - mineRank(b) || a.x - b.x);
  const light: PlacedLabel[] = [];
  for (const m of inaccuracies) {
    const text = textFor(m, form);
    const w = labelWidth(text);
    const x = Math.min(Math.max(m.x - 6, 0), width - w);
    const clearOf = (ls: PlacedLabel[], gap: number) =>
      ls.every((t) => x + w + gap <= t.x || x >= t.x + t.width + gap);
    if (!clearOf(placed, SERIOUS_CLEARANCE) || !clearOf(light, LABEL_GAP)) continue;
    if (light.some((t) => Math.abs(t.x - x) < INACCURACY_SPACING)) continue;
    light.push({ ply: m.ply, x, width: w, tickX: m.x, serious: false, leader: true, text });
  }
  return [...placed, ...light];
}
