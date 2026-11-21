// Pen stroke geometry (DESIGN.md, "Pen stroke geometry"; design/decisions/board.md, "Stroke" and
// "Arrowhead geometry"). Everything is in board units: one square is 1, the board is 8 by 8 and
// (0, 0) is its top-left corner as drawn. DESIGN.md gives px at a 56px square, so those are
// divided by 56 here. This folder is one of the places raw geometry is allowed.

import { type Orientation, type Square, squareToCell } from "../board/theme.js";
import { formatPath, type PathCommand, parsePath } from "./path.js";
import { roughenCommands, seededRandom } from "./roughen.js";
import type { Point, Tone } from "./types.js";

const PX = 1 / 56;

/** The width profile at a 56px square: 2.2 at the start, 3.2 from 35% to 65%, 1.4 at the end. */
export const WIDTH = { start: 2.2 * PX, mid: 3.2 * PX, end: 1.4 * PX } as const;
/** Width by tone: pen and good 1.0, pencil 0.7. */
export const TONE_SCALE: Readonly<Record<Tone, number>> = { pen: 1, good: 1, pencil: 0.7 };
/** Arrowhead strokes are 0.9 of their shaft. */
export const HEAD_SCALE = 0.9;
/** The `sheet` halo, each side of the outline (a 4px stroke under the fill at 56px). */
export const HALO = 2 * PX;

export const ARROW = {
  /** the shaft starts this far from the origin centre */
  start: 0.22,
  /** and ends this far short of a bare target's centre */
  end: 0.16,
  /** or this far short when the target carries a ring: the tip stops on its outer edge */
  endRinged: 0.46,
  /** the open head's base length; its strokes are 0.9 and 1.1 of it */
  head: 0.3,
  /** the two head strokes' angles to the shaft */
  angles: [26, 32] as const,
  /** a knight's L rounds its corner over this radius, as a hand does */
  corner: 0.14,
} as const;

export const RING = {
  ry: 0.38,
  ratio: 1.12,
  /** tilt in degrees, seeded within this range, either way */
  tilt: [8, 12] as const,
  /** where the stroke starts, from three o'clock in the ellipse's frame (about 1 o'clock) */
  startDeg: -60,
  sweepDeg: 380,
  /** outward growth per revolution, so the overrun sits outside the start */
  growth: 2.5 * PX,
  stepDeg: 5,
} as const;

/** The badge (DESIGN.md, "Glyph badge"): radius 20% of the square, inset 1px. */
export const BADGE = { r: 0.2, font1: 0.35, font2: 0.28, minFontPx: 11 } as const;

/** The centre of a square, as drawn in the given orientation. */
export function squareCenter(sq: Square, orientation: Orientation): Point {
  const { col, row } = squareToCell(sq, orientation);
  return { x: col + 0.5, y: row + 0.5 };
}

export interface ArrowOptions {
  /** the target carries a ring, so the shaft stops on its outer edge (ruling 1, Task 10) */
  ringed?: boolean;
  /** `good` gets a solid head; `pen` and `pencil` an open two-stroke head */
  tone?: Tone;
  /** a seed bows the shaft (as `roughen` does) and picks which side gets which head stroke */
  seed?: string;
  /** which leg of a knight's L comes first: the two-square leg (default) or the one-square leg */
  knight?: "long" | "short";
}

const unit = (a: Point, b: Point): Point => {
  const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  return { x: (b.x - a.x) / len, y: (b.y - a.y) / len };
};

/** True when the move is a knight's jump (one square one way, two the other). */
export function isKnightMove(from: Square, to: Square): boolean {
  const df = Math.abs(from.charCodeAt(0) - to.charCodeAt(0));
  const dr = Math.abs(Number(from[1]) - Number(to[1]));
  return (df === 1 && dr === 2) || (df === 2 && dr === 1);
}

/** The corner of a knight's L, as drawn. */
export function knightCorner(
  from: Square,
  to: Square,
  orientation: Orientation,
  first: "long" | "short" = "long",
): Point {
  const a = squareCenter(from, orientation);
  const b = squareCenter(to, orientation);
  const horizontalLong = Math.abs(b.x - a.x) > Math.abs(b.y - a.y);
  const horizontalFirst = first === "long" ? horizontalLong : !horizontalLong;
  return horizontalFirst ? { x: b.x, y: a.y } : { x: a.x, y: b.y };
}

/**
 * An arrow's shaft and head as paths. The shaft is straight (one segment), or an L of two straight
 * segments for a knight's move; with a `seed` each segment bows 2–4% as a quadratic. The head
 * follows the shaft's end tangent: two separate strokes for an open head (`M L M L`), or one closed
 * triangle for a solid one.
 */
export function arrowGeometry(
  from: Square,
  to: Square,
  orientation: Orientation,
  options: ArrowOptions = {},
): { shaft: string; head: string } {
  const { ringed = false, tone = "pen", seed, knight = "long" } = options;
  const a = squareCenter(from, orientation);
  const b = squareCenter(to, orientation);
  const corner = isKnightMove(from, to) ? knightCorner(from, to, orientation, knight) : null;
  const first = unit(a, corner ?? b);
  const last = unit(corner ?? a, b);
  const clear = ringed ? ARROW.endRinged : ARROW.end;
  const start = { x: a.x + first.x * ARROW.start, y: a.y + first.y * ARROW.start };
  const tip = { x: b.x - last.x * clear, y: b.y - last.y * clear };
  let cmds: PathCommand[] = [
    { type: "M", ...start },
    ...(corner ? [{ type: "L" as const, ...corner }] : []),
    { type: "L", ...tip },
  ];
  let swap = false;
  if (seed !== undefined) {
    // Draw order (DESIGN.md, "Determinism"): bow and bow side per segment, then the head side.
    const rand = seededRandom(seed);
    cmds = roughenCommands(cmds, rand);
    swap = rand() < 0.5;
  }
  const end = cmds[cmds.length - 1] as Extract<PathCommand, { type: "L" | "Q" }>;
  const before = cmds[cmds.length - 2] as Extract<PathCommand, { x: number }>;
  const from2 = end.type === "Q" ? { x: end.cx, y: end.cy } : { x: before.x, y: before.y };
  const back = Math.atan2(from2.y - tip.y, from2.x - tip.x);
  const side = swap ? -1 : 1;
  const stroke = (deg: number, k: number): Point => {
    const r = back + (deg * Math.PI) / 180;
    return { x: tip.x + Math.cos(r) * ARROW.head * k, y: tip.y + Math.sin(r) * ARROW.head * k };
  };
  const p1 = stroke(side * ARROW.angles[0], 0.9);
  const p2 = stroke(-side * ARROW.angles[1], 1.1);
  const head: PathCommand[] =
    tone === "good"
      ? [{ type: "M", ...p1 }, { type: "L", ...tip }, { type: "L", ...p2 }, { type: "Z" }]
      : [
          { type: "M", ...tip },
          { type: "L", ...p1 },
          { type: "M", ...tip },
          { type: "L", ...p2 },
        ];
  return { shaft: formatPath(cmds), head: formatPath(head) };
}

/** The ring's centreline as points every 5°, from its start to 380°. */
export function ringPoints(sq: Square, orientation: Orientation, seed: string): Point[] {
  const c = squareCenter(sq, orientation);
  // Draw order: tilt, tilt sign, φ₁, φ₂.
  const rand = seededRandom(seed);
  const tiltDeg = (RING.tilt[0] + (RING.tilt[1] - RING.tilt[0]) * rand()) * (rand() < 0.5 ? -1 : 1);
  const phi1 = rand() * 2 * Math.PI;
  const phi2 = rand() * 2 * Math.PI;
  const tilt = (tiltDeg * Math.PI) / 180;
  const pts: Point[] = [];
  for (let d = 0; d <= RING.sweepDeg; d += RING.stepDeg) {
    // Counter-clockwise as drawn (y grows downwards), as a right hand rings a square.
    const th = ((RING.startDeg - d) * Math.PI) / 180;
    // Integer frequencies, so the drift at θ and θ + 360° is the same and only the growth differs.
    const drift = 1 + 0.04 * Math.sin(2 * th + phi1) + 0.02 * Math.sin(3 * th + phi2);
    const grow = (RING.growth * d) / 360;
    const ex = (RING.ry * RING.ratio * drift + grow) * Math.cos(th);
    const ey = (RING.ry * drift + grow) * Math.sin(th);
    pts.push({
      x: c.x + ex * Math.cos(tilt) - ey * Math.sin(tilt),
      y: c.y + ex * Math.sin(tilt) + ey * Math.cos(tilt),
    });
  }
  return pts;
}

/** The ring as a path: a tilted, drifting ellipse from about 1 o'clock, spiralling out to 380°. */
export function circlePath(sq: Square, orientation: Orientation, seed: string): string {
  return polyline(ringPoints(sq, orientation, seed));
}

export function polyline(pts: readonly Point[]): string {
  return formatPath(pts.map((p, i) => ({ type: i === 0 ? "M" : "L", x: p.x, y: p.y })));
}

const SAMPLE = 0.04;

/**
 * Samples a path into a dense polyline, one point about every 0.04 of a square. Where two
 * segments meet at a turn of more than 20° (a knight's L), the corner is rounded over
 * `ARROW.corner`. Only the first subpath is sampled.
 */
export function samplePath(d: string): Point[] {
  const cmds = parsePath(d);
  const pts: Point[] = [];
  const corners: number[] = [];
  let at: Point | null = null;
  for (const c of cmds) {
    if (c.type === "M") {
      if (at) break;
      at = { x: c.x, y: c.y };
      pts.push(at);
      continue;
    }
    if (c.type === "Z" || c.type === "A" || !at) break;
    if (pts.length > 1) corners.push(pts.length - 1);
    const p0: Point = at;
    const ctrl = c.type === "Q" ? { x: c.cx, y: c.cy } : null;
    const len = Math.hypot(c.x - p0.x, c.y - p0.y);
    const steps = Math.max(1, Math.ceil(len / SAMPLE));
    for (let i = 1; i <= steps; i++) {
      const t = i / steps;
      if (ctrl) {
        const u = 1 - t;
        pts.push({
          x: u * u * p0.x + 2 * u * t * ctrl.x + t * t * c.x,
          y: u * u * p0.y + 2 * u * t * ctrl.y + t * t * c.y,
        });
      } else pts.push({ x: p0.x + (c.x - p0.x) * t, y: p0.y + (c.y - p0.y) * t });
    }
    at = { x: c.x, y: c.y };
  }
  for (const k of corners.reverse()) fillet(pts, k);
  return pts;
}

/** Rounds the polyline's corner at index `k` with a quadratic through it, in place. */
function fillet(pts: Point[], k: number) {
  const c = pts[k] as Point;
  const a = pts[k - 1] as Point;
  const b = pts[k + 1] as Point;
  const turn = Math.abs(
    Math.atan2(
      (c.x - a.x) * (b.y - c.y) - (c.y - a.y) * (b.x - c.x),
      (c.x - a.x) * (b.x - c.x) + (c.y - a.y) * (b.y - c.y),
    ),
  );
  if (turn < (20 * Math.PI) / 180) return;
  let i = k;
  while (i > 0 && Math.hypot(c.x - (pts[i] as Point).x, c.y - (pts[i] as Point).y) < ARROW.corner)
    i--;
  let j = k;
  while (
    j < pts.length - 1 &&
    Math.hypot(c.x - (pts[j] as Point).x, c.y - (pts[j] as Point).y) < ARROW.corner
  )
    j++;
  const p0 = pts[i] as Point;
  const p2 = pts[j] as Point;
  const curve: Point[] = [];
  const steps = 8;
  for (let s = 1; s < steps; s++) {
    const t = s / steps;
    const u = 1 - t;
    curve.push({
      x: u * u * p0.x + 2 * u * t * c.x + t * t * p2.x,
      y: u * u * p0.y + 2 * u * t * c.y + t * t * p2.y,
    });
  }
  pts.splice(i + 1, j - i - 1, ...curve);
}

const smooth = (t: number) => t * t * (3 - 2 * t);

/** The full outline width at `t` (0 to 1 along the stroke), at a 56px square, times `scale`. */
export function widthAt(t: number, scale = 1): number {
  if (t < 0.35) return scale * (WIDTH.start + (WIDTH.mid - WIDTH.start) * smooth(t / 0.35));
  if (t <= 0.65) return scale * WIDTH.mid;
  return scale * (WIDTH.mid + (WIDTH.end - WIDTH.mid) * smooth((t - 0.65) / 0.35));
}

export interface OutlineOptions {
  /** the narrowest the outline may get, in board units (DESIGN.md: 1.1px) */
  minWidth?: number;
  /** the part of the whole stroke this piece is, so a split stroke keeps one width profile */
  span?: readonly [number, number];
}

/**
 * The filled outline of a variable-width stroke around `d`'s centreline: the left edge forward,
 * a round cap, the right edge back and a round cap. Never a constant-width stroke.
 */
export function strokeOutline(d: string, scale: number, options: OutlineOptions = {}): string {
  return outlinePoints(samplePath(d), scale, options);
}

export function outlinePoints(
  pts: readonly Point[],
  scale: number,
  { minWidth = 0, span = [0, 1] }: OutlineOptions = {},
): string {
  const n = pts.length;
  const lengths = [0];
  for (let i = 1; i < n; i++) {
    const p = pts[i] as Point;
    const q = pts[i - 1] as Point;
    lengths.push((lengths[i - 1] as number) + Math.hypot(p.x - q.x, p.y - q.y));
  }
  const total = (lengths[n - 1] as number) || 1;
  const left: Point[] = [];
  const right: Point[] = [];
  const halves: number[] = [];
  for (let i = 0; i < n; i++) {
    const p = pts[i] as Point;
    const a = pts[Math.max(0, i - 1)] as Point;
    const b = pts[Math.min(n - 1, i + 1)] as Point;
    const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const t = span[0] + (span[1] - span[0]) * ((lengths[i] as number) / total);
    const half = Math.max(minWidth, widthAt(t, scale)) / 2;
    halves.push(half);
    const nx = -(b.y - a.y) / len;
    const ny = (b.x - a.x) / len;
    left.push({ x: p.x + nx * half, y: p.y + ny * half });
    right.push({ x: p.x - nx * half, y: p.y - ny * half });
  }
  const cmds: PathCommand[] = [
    { type: "M", ...(left[0] as Point) },
    ...left.slice(1).map((p) => ({ type: "L" as const, ...p })),
    { type: "A", r: halves[n - 1] as number, sweep: 1, ...(right[n - 1] as Point) },
    ...right
      .slice(0, -1)
      .reverse()
      .map((p) => ({ type: "L" as const, ...p })),
    { type: "A", r: halves[0] as number, sweep: 1, ...(left[0] as Point) },
    { type: "Z" },
  ];
  return formatPath(cmds);
}

/** The widest the outline gets, for sizing the reveal mask's stroke. */
export function maxWidth(scale: number, minWidth = 0): number {
  return Math.max(minWidth, WIDTH.mid * scale);
}

/**
 * The real-time fraction of a cubic-bezier ease at which its progress reaches `progress`
 * (DESIGN.md M2, M3: a shaft reaches 85% at 63% of its duration).
 */
export function timeAtProgress(
  e: readonly [number, number, number, number],
  progress: number,
): number {
  const [x1, y1, x2, y2] = e;
  const at = (a: number, b: number, s: number) =>
    3 * (1 - s) * (1 - s) * s * a + 3 * (1 - s) * s * s * b + s * s * s;
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 50; i++) {
    const mid = (lo + hi) / 2;
    if (at(y1, y2, mid) < progress) lo = mid;
    else hi = mid;
  }
  return at(x1, x2, (lo + hi) / 2);
}
