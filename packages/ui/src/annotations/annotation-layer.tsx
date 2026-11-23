"use client";

import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";

import { cellToSquare, coordinateBoxes, squareToCell } from "../board/theme.js";
import {
  animate,
  dur,
  ease,
  type MotionValue,
  motion,
  motionValue,
  spring,
  stagger,
  useReducedMotion,
  useTransform,
} from "../motion/index.js";
import {
  arrowGeometry,
  BADGE,
  HALO,
  HALO_SCALE,
  HEAD_SCALE,
  isKnightMove,
  type KnightRoute,
  knightCorner,
  knightPath,
  maxWidth,
  outlinePoints,
  polyline,
  ringPoints,
  STEP_BADGE,
  samplePath,
  TONE_SCALE,
  timeAtProgress,
} from "./geometry.js";
import { parsePath } from "./path.js";
import type { Annotation, AnnotationLayerProps, Orientation, Point, Tone } from "./types.js";

// The red-pen overlay (DESIGN.md, "Severity and marks", "Pen stroke geometry", M2, M3, M10).
// Every stroke is a filled, tapered outline, painted twice (the `sheet` halo, then the fill) and
// revealed through a mask whose centreline draws on by `pathLength`.

const EMPTY: ReadonlySet<string> = new Set();

const TONE_COLOUR: Readonly<Record<Tone, string>> = {
  pen: "var(--color-pen)",
  good: "var(--color-good)",
  pencil: "var(--color-pencil)",
};

const TONE_WORD: Readonly<Record<Tone, string>> = { pen: "Red", good: "Green", pencil: "Grey" };

/** What assistive tech hears for a mark: "Red arrow from e4 to e6", "Green circle on f7". */
export function describeAnnotation(a: Annotation): string {
  const tone = TONE_WORD[a.tone];
  if (a.kind === "arrow") {
    const what = `${tone} arrow from ${a.from} to ${a.to}`;
    return a.step ? `${a.step}: ${what}` : what;
  }
  if (a.kind === "circle") return `${tone} circle on ${a.square}`;
  return `${tone} ${a.glyph} on ${a.square}`;
}

/** The time a head stroke starts: when its shaft reaches 85% of its eased progress (M3). */
const HEAD_AT = timeAtProgress(ease.pen, 0.85) * dur.draw;
/** The second head stroke starts 30ms after the first (M3). */
const HEAD_GAP = 0.03;

interface Track {
  delay: number;
  duration: number;
}

interface Segment {
  /** the filled outline */
  outline: string;
  /** the centreline the reveal mask draws along */
  centre: string;
  /** the reveal mask's stroke width */
  maskWidth: number;
  maskCap: "round" | "butt";
  track: number;
  /** the part of the track's progress this segment draws over */
  span: readonly [number, number];
}

interface StrokeMark {
  kind: "arrow" | "circle";
  /** an arrow's knight route (`long` for any other arrow) */
  route?: KnightRoute;
  key: string;
  tone: Tone;
  /** the halo's full stroke width (both sides) */
  halo: number;
  /** each group paints all its halos, then all its fills, so a group reads as one stroke */
  groups: Segment[][];
  tracks: Track[];
}

interface BadgeMark {
  kind: "mark" | "step";
  key: string;
  tone: Tone;
  weight: "full" | "light";
  glyph: string;
  cx: number;
  cy: number;
  r: number;
  fontSize: number;
  at: number;
}

type Plan = StrokeMark | BadgeMark;

const pts = (d: string): Point[] =>
  parsePath(d).flatMap((c) => (c.type === "M" || c.type === "L" ? [{ x: c.x, y: c.y }] : []));

/** A mark's identity: the same arrow, ring or badge twice is drawn and read once. */
export function annotationKey(a: Annotation): string {
  if (a.kind === "arrow") return `arrow:${a.from}${a.to}:${a.tone}${a.step ? `:${a.step}` : ""}`;
  if (a.kind === "circle") return `circle:${a.square}:${a.tone}`;
  return `mark:${a.square}:${a.glyph}:${a.tone}`;
}

function unique(annotations: readonly Annotation[]): Annotation[] {
  const seen = new Set<string>();
  return annotations.filter((a) => {
    const key = annotationKey(a);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function planMarks(
  annotations: readonly Annotation[],
  orientation: Orientation,
  squarePx: number,
  occupied: ReadonlySet<string> = EMPTY,
): Plan[] {
  const px = 1 / squarePx;
  const minWidth = 1.1 * px;
  const haloOf = (tone: Tone) => 2 * HALO * HALO_SCALE[tone];
  const ringed = new Set(annotations.flatMap((a) => (a.kind === "circle" ? [a.square] : [])));
  const arrows = annotations.filter(
    (a): a is Extract<Annotation, { kind: "arrow" }> => a.kind === "arrow",
  );
  const plans: Plan[] = [];
  const steps: BadgeMark[] = [];
  /** For each target, the points of every arrowhead that lands on it, to place the badge. */
  const arrivals = new Map<string, Point[]>();
  let strokeIndex = 0;
  let lastEnd = 0;

  for (const a of annotations) {
    const scale = TONE_SCALE[a.tone];
    const pad = haloOf(a.tone) + px;
    if (a.kind === "arrow") {
      const delay = strokeIndex++ * stagger.pen;
      // A knight's L runs its long leg first, unless that corner sits on another arrow's end or
      // the L would cross a piece (T13 critic); then the short leg first, and when both cross a
      // piece, a straight shaft between them.
      let knight: KnightRoute = "long";
      if (isKnightMove(a.from, a.to)) {
        const c = knightCorner(a.from, a.to, orientation, "long");
        const sq = cellToSquare(Math.floor(c.x), Math.floor(c.y), orientation);
        const onArrowEnd = arrows.some((o) => o !== a && (o.from === sq || o.to === sq));
        const crosses = (first: "long" | "short") =>
          knightPath(a.from, a.to, first).some((s) => occupied.has(s));
        if (onArrowEnd || crosses("long")) knight = crosses("short") ? "straight" : "short";
      }
      const { shaft, head } = arrowGeometry(a.from, a.to, orientation, {
        ringed: ringed.has(a.to) || a.toEdge === true,
        tone: a.tone,
        seed: `arrow:${a.from}${a.to}`,
        knight,
      });
      const line = samplePath(shaft);
      const segments: Segment[] = [
        {
          outline: outlinePoints(line, scale, { minWidth }),
          centre: polyline(line),
          maskWidth: maxWidth(scale, minWidth) + pad,
          maskCap: "round",
          track: 0,
          span: [0, 1],
        },
      ];
      const tracks: Track[] = [{ delay, duration: dur.draw }];
      const headCmds = parsePath(head);
      const tip = { x: (headCmds[0] as Point).x, y: (headCmds[0] as Point).y };
      arrivals.set(a.to, [...(arrivals.get(a.to) ?? []), ...pts(head)]);
      if (a.tone === "good") {
        // A solid head: the triangle on its halo, wiped on from its base to its tip.
        const [p1, t, p2] = pts(head) as [Point, Point, Point];
        const base = { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
        const back = { x: base.x + (base.x - t.x) * 0.15, y: base.y + (base.y - t.y) * 0.15 };
        const width = Math.hypot(p1.x - p2.x, p1.y - p2.y) + pad * 2;
        segments.push({
          outline: `${head}`,
          centre: polyline([
            back,
            { x: t.x + (t.x - base.x) * 0.1, y: t.y + (t.y - base.y) * 0.1 },
          ]),
          maskWidth: width,
          maskCap: "butt",
          track: 1,
          span: [0, 1],
        });
        tracks.push({ delay: delay + HEAD_AT, duration: dur.instant });
      } else {
        const ends = pts(head).filter((_, i) => i % 2 === 1);
        ends.forEach((end, i) => {
          const stroke = [tip, end];
          const dense = samplePath(polyline(stroke));
          segments.push({
            outline: outlinePoints(dense, scale * HEAD_SCALE, { minWidth }),
            centre: polyline(dense),
            maskWidth: maxWidth(scale * HEAD_SCALE, minWidth) + pad,
            maskCap: "round",
            track: i + 1,
            span: [0, 1],
          });
          tracks.push({ delay: delay + HEAD_AT + i * HEAD_GAP, duration: dur.instant });
        });
      }
      lastEnd = Math.max(lastEnd, ...tracks.map((t) => t.delay + t.duration));
      if (a.step && squarePx >= STEP_BADGE.minSquarePx) {
        const font = Math.max(STEP_BADGE.font, STEP_BADGE.minFontPx * px);
        const r = Math.max(STEP_BADGE.r, font * 0.6);
        const at = stepPoint(line, r, steps);
        steps.push({
          kind: "step",
          key: `step:${a.from}${a.to}:${a.step}`,
          tone: a.tone,
          weight: "full",
          glyph: String(a.step),
          cx: at.x,
          cy: at.y,
          r,
          fontSize: font,
          at: delay + dur.draw,
        });
      }
      plans.push({
        kind: "arrow",
        route: knight,
        key: `arrow:${a.from}${a.to}:${a.tone}`,
        tone: a.tone,
        halo: haloOf(a.tone),
        groups: [segments],
        tracks,
      });
    } else if (a.kind === "circle") {
      const delay = strokeIndex++ * stagger.pen;
      const ring = ringPoints(a.square, orientation, `ring:${a.square}`);
      // Points are every 5°: 0–180° and 180–360° are one stroke in two pieces (so the mask of one
      // never uncovers the other), and the 20° overrun is its own stroke that draws over the start.
      const steps = ring.length - 1;
      const cut1 = Math.round((steps * 180) / 380);
      const cut2 = Math.round((steps * 360) / 380);
      const pieces = [ring.slice(0, cut1 + 1), ring.slice(cut1, cut2 + 1), ring.slice(cut2)];
      const bounds = [0, cut1 / steps, cut2 / steps, 1];
      const segs = pieces.map(
        (piece, i): Segment => ({
          outline: outlinePoints(piece, scale, {
            minWidth,
            span: [bounds[i] as number, bounds[i + 1] as number],
          }),
          centre: polyline(piece),
          maskWidth: maxWidth(scale, minWidth) + pad,
          maskCap: "round",
          track: 0,
          span: [bounds[i] as number, bounds[i + 1] as number],
        }),
      );
      lastEnd = Math.max(lastEnd, delay + dur.draw);
      plans.push({
        kind: "circle",
        key: `circle:${a.square}:${a.tone}`,
        tone: a.tone,
        halo: haloOf(a.tone),
        // One group: the overrun inks over the start, as a pen does, with no halo cut between.
        groups: [segs],
        tracks: [{ delay, duration: dur.draw }],
      });
    }
  }

  // Badges land when the last stroke ends, not by stagger (DESIGN.md, "Stagger scale").
  for (const a of annotations) {
    if (a.kind !== "mark") continue;
    // The corner farthest from every arrowhead that lands on the square (Task 12 review: also
    // from above and from the diagonals), preferring top right, then top left, bottom right and
    // bottom left. A corner that holds a coordinate (rank numbers top left of the left column,
    // file letters bottom right of the bottom row) is never used.
    const { col, row } = squareToCell(a.square, orientation);
    const r = BADGE.r;
    const inset = px;
    const left = col + inset + r;
    const right = col + 1 - inset - r;
    const top = row + inset + r;
    const bottom = row + 1 - inset - r;
    // `edge`: on the square's top corners themselves, kept on the board. On the left column the
    // top-left corner holds the rank number (M5a T13, T10 m1), so the fallback there is the
    // square's own bottom-right corner, unless that holds a file letter (the bottom row).
    const onBoard = (v: number) => Math.min(8 - inset - r, Math.max(inset + r, v));
    const corners =
      a.place === "edge"
        ? [
            { x: onBoard(col + 1), y: onBoard(row), ok: true },
            col !== 0
              ? { x: onBoard(col), y: onBoard(row), ok: true }
              : { x: right, y: bottom, ok: row !== 7 },
          ].filter((c) => c.ok)
        : [
            { x: right, y: top, ok: true },
            { x: left, y: top, ok: col !== 0 },
            { x: right, y: bottom, ok: row !== 7 },
            { x: left, y: bottom, ok: true },
          ].filter((c) => c.ok);
    const heads = arrivals.get(a.square) ?? [];
    const clearance = (c: Point) =>
      heads.reduce(
        (m, q) => Math.min(m, Math.hypot(q.x - c.x, q.y - c.y)),
        Number.POSITIVE_INFINITY,
      );
    let best = corners[0] as Point;
    let bestScore = clearance(best);
    for (const c of corners.slice(1)) {
      const score = clearance(c);
      if (score > bestScore + 0.02) {
        best = c;
        bestScore = score;
      }
    }
    const cx = best.x;
    const cy = best.y;
    const font = Math.max(a.glyph.length === 1 ? BADGE.font1 : BADGE.font2, BADGE.minFontPx * px);
    plans.push({
      kind: "mark",
      key: `mark:${a.square}:${a.glyph}:${a.tone}`,
      tone: a.tone,
      weight: a.weight ?? "full",
      glyph: a.glyph,
      cx,
      cy,
      r,
      fontSize: font,
      at: lastEnd,
    });
  }
  // Steps in move order, whatever order the arrows came in.
  plans.push(...steps.sort((x, y) => Number(x.glyph) - Number(y.glyph)));
  return plans;
}

/**
 * Where a step number sits on its shaft: in the shaft's first half, toward the tail, never over
 * the arrowhead (owner: "don't put the numbers over the tip of the arrow"). The sampled point
 * nearest `STEP_AT` of the way along that is off every square's centre (so never on a piece) and
 * clear of the badges already placed; with none, the point at `STEP_AT` itself.
 */
function stepPoint(line: Point[], r: number, placed: readonly BadgeMark[]): Point {
  const lengths = [0];
  for (let i = 1; i < line.length; i++) {
    const a = line[i - 1] as Point;
    const b = line[i] as Point;
    lengths.push((lengths[i - 1] as number) + Math.hypot(b.x - a.x, b.y - a.y));
  }
  const total = lengths[lengths.length - 1] || 1;
  const frac = (v: number) => v - Math.floor(v);
  const offCentre = (p: Point) => Math.hypot(frac(p.x) - 0.5, frac(p.y) - 0.5) >= 0.3;
  const clear = (p: Point) =>
    placed.every((b) => Math.hypot(b.cx - p.x, b.cy - p.y) >= b.r + r + 0.04);
  let best: Point | null = null;
  let bestGap = Number.POSITIVE_INFINITY;
  line.forEach((p, i) => {
    const t = (lengths[i] as number) / total;
    if (t < STEP_SPAN[0] || t > STEP_SPAN[1] || !offCentre(p) || !clear(p)) return;
    const gap = Math.abs(t - STEP_AT);
    if (gap < bestGap) {
      best = p;
      bestGap = gap;
    }
  });
  if (best) return best;
  return pointAt(line, lengths, STEP_AT * total);
}

/** A step number's place along its shaft: a third of the way, between 10% and 50%. */
export const STEP_AT = 0.33;
export const STEP_SPAN = [0.1, 0.5] as const;

/** The point `d` along the sampled polyline `line` (cumulative `lengths`). */
function pointAt(line: Point[], lengths: number[], d: number): Point {
  for (let i = 1; i < line.length; i++) {
    const end = lengths[i] as number;
    if (end >= d) {
      const a = line[i - 1] as Point;
      const b = line[i] as Point;
      const start = lengths[i - 1] as number;
      const t = end > start ? (d - start) / (end - start) : 0;
      return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
    }
  }
  return line[0] ?? { x: 4, y: 4 };
}

/** One square's width in CSS px, measured; 56 (DESIGN.md's reference square) until known. */
function useSquarePx(ref: { current: SVGSVGElement | null }): number {
  const [px, setPx] = useState(56);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const read = () => {
      const w = el.getBoundingClientRect().width;
      if (w > 0) setPx(Math.round((w / 8) * 4) / 4);
    };
    read();
    const observer = new ResizeObserver(read);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return px;
}

function Reveal({ segment, progress }: { segment: Segment; progress: MotionValue<number> }) {
  const [lo, hi] = segment.span;
  const pathLength = useTransform(progress, (v) =>
    Math.min(1, Math.max(0, (v - lo) / (hi - lo || 1))),
  );
  // A dash of length 0 with a round cap is still a dot, so a segment shows once it has begun.
  const opacity = useTransform(progress, (v) => (v > lo || v >= 1 ? 1 : 0));
  return (
    <motion.path
      data-reveal
      d={segment.centre}
      fill="none"
      stroke="white"
      strokeWidth={segment.maskWidth}
      strokeLinecap={segment.maskCap}
      strokeLinejoin="round"
      style={{ pathLength, opacity }}
    />
  );
}

/**
 * One paint pass of a stroke mark: its halos, its fills, or both (each group's halos first). With
 * no `maskId` nothing animates, and the paths are plain (no masks).
 */
function MarkPaint({
  mark,
  part,
  maskId,
}: {
  mark: StrokeMark;
  part: "halo" | "fill" | "both";
  maskId?: ((segment: number) => string) | undefined;
}) {
  const colour = TONE_COLOUR[mark.tone];
  const masked = (i: number) => (maskId ? { mask: `url(#${maskId(i)})` } : {});
  let index = 0;
  return (
    <g data-mark={mark.kind} data-tone={mark.tone} data-route={mark.route}>
      {mark.groups.map((group, g) => {
        const first = index;
        index += group.length;
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed group order
          <g key={g}>
            {part !== "fill"
              ? group.map((s, i) => (
                  <path
                    // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
                    key={`h${i}`}
                    data-part="halo"
                    d={s.outline}
                    {...masked(first + i)}
                    fill="var(--color-sheet)"
                    stroke="var(--color-sheet)"
                    strokeWidth={mark.halo}
                    strokeLinejoin="round"
                  />
                ))
              : null}
            {part !== "halo"
              ? group.map((s, i) => (
                  <path
                    // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
                    key={`f${i}`}
                    data-part="fill"
                    d={s.outline}
                    {...masked(first + i)}
                    fill={colour}
                  />
                ))
              : null}
          </g>
        );
      })}
    </g>
  );
}

/** A light badge's ring, in board units (2px at the 56px reference square). */
const LIGHT_RING = 2 / 56;

function BadgeView({ mark, still }: { mark: BadgeMark; still: boolean }) {
  const colour = TONE_COLOUR[mark.tone];
  const light = mark.weight === "light";
  return (
    <motion.g
      data-mark={mark.kind === "step" ? "step" : "badge"}
      data-tone={mark.tone}
      data-weight={mark.weight}
      style={{ transformBox: "fill-box", transformOrigin: "center" }}
      initial={still ? false : { scale: 0, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      transition={
        still
          ? { duration: 0 }
          : {
              scale: { ...spring.land, delay: mark.at },
              opacity: { duration: dur.instant, delay: mark.at },
            }
      }
    >
      <circle cx={mark.cx} cy={mark.cy} r={mark.r + HALO} fill="var(--color-sheet)" />
      {light ? (
        <circle
          cx={mark.cx}
          cy={mark.cy}
          r={mark.r - LIGHT_RING / 2}
          fill="var(--color-sheet)"
          stroke={colour}
          strokeWidth={LIGHT_RING}
        />
      ) : (
        <circle cx={mark.cx} cy={mark.cy} r={mark.r} fill={colour} />
      )}
      <text
        x={mark.cx}
        y={mark.cy}
        textAnchor="middle"
        dominantBaseline="central"
        fill={light ? colour : "var(--color-on-ink)"}
        style={{
          fontFamily: "var(--font-serif)",
          fontWeight: 600,
          fontSize: mark.fontSize,
          letterSpacing: mark.glyph.length > 1 ? -0.01 : 0,
        }}
      >
        {mark.glyph}
      </text>
    </motion.g>
  );
}

interface Running {
  key: string;
  stop: () => void;
  value: MotionValue<number>;
}

/**
 * The annotation overlay: an SVG in board units laid over the board, and a visually hidden list
 * that says what each mark is. Strokes draw on in order, `stagger.pen` apart, each over `dur.draw`
 * with `ease.pen`; arrowheads start at 85% of their shaft; badges land on `land` when the last
 * stroke ends. With `animate={false}` or reduced motion, everything is drawn at once, as plain
 * paths with no masks. Duplicate annotations are drawn and read once.
 *
 * Paint order: ring halos, then each arrow (its halos, then its fills), then ring fills. Arrows
 * still cut each other with their halos where they cross, but no arrowhead's halo notches a ring.
 */
export function AnnotationLayer({
  annotations: given,
  orientation,
  animate: animated = true,
  occupied,
}: AnnotationLayerProps) {
  const reduced = useReducedMotion();
  const still = !animated || reduced;
  const ref = useRef<SVGSVGElement>(null);
  const squarePx = useSquarePx(ref);
  const id = `ann-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const annotations = useMemo(() => unique(given), [given]);
  const plans = useMemo(
    () => planMarks(annotations, orientation, squarePx, occupied),
    [annotations, orientation, squarePx, occupied],
  );
  const strokes = plans.filter((p): p is StrokeMark => p.kind === "arrow" || p.kind === "circle");
  const badges = plans.filter((p): p is BadgeMark => p.kind === "mark" || p.kind === "step");

  // One progress value per track, per mark; a ring's three pieces share one, split by arc (M2).
  // They live here, not in the marks, because a mark is painted in more than one pass.
  const progress = useRef(new Map<string, MotionValue<number>[]>()).current;
  const tracksOf = (mark: StrokeMark): MotionValue<number>[] => {
    let values = progress.get(mark.key);
    if (!values) {
      values = mark.tracks.map(() => motionValue(still ? 1 : 0));
      progress.set(mark.key, values);
    }
    return values;
  };
  const tracksRef = useRef(tracksOf);
  tracksRef.current = tracksOf;
  // The timing never depends on the square size, so a resize re-plans without replaying.
  const timing = useRef(strokes);
  timing.current = strokes;
  const drawn = useRef(new Set<string>()).current;
  const keys = strokes.map((m) => m.key).join("|");
  // biome-ignore lint/correctness/useExhaustiveDependencies: `keys` stands for the marks
  useEffect(() => {
    const running: Running[] = [];
    for (const mark of timing.current) {
      const values = tracksRef.current(mark);
      if (still) {
        for (const v of values) v.set(1);
        continue;
      }
      if (drawn.has(mark.key)) continue;
      drawn.add(mark.key);
      values.forEach((value, i) => {
        const track = mark.tracks[i] as Track;
        value.set(0);
        const controls = animate(value, 1, {
          delay: track.delay,
          duration: track.duration,
          ease: [...ease.pen],
        });
        running.push({ key: mark.key, stop: () => controls.stop(), value });
      });
    }
    return () => {
      // An unfinished stroke draws again next time (and under StrictMode's double effect).
      for (const r of running) {
        r.stop();
        if (r.value.get() < 1) drawn.delete(r.key);
      }
    };
  }, [still, keys, drawn]);

  // Marks never swallow a coordinate (DESIGN.md, "The board"): strokes are cut where the glyphs
  // sit (their boxes plus 2px). Ranks are top left of the left column, files bottom right of the
  // bottom row, as drawn.
  const cuts = useMemo(() => {
    const box = coordinateBoxes(squarePx);
    const pad = 2 / squarePx;
    const rects: { x: number; y: number; w: number; h: number }[] = [];
    for (let i = 0; i < 8; i++) {
      rects.push({
        x: box.rank.left - pad,
        y: i + box.rank.top - pad,
        w: box.rank.right - box.rank.left + 2 * pad,
        h: box.rank.bottom - box.rank.top + 2 * pad,
      });
      rects.push({
        x: i + box.file.left - pad,
        y: 7 + box.file.top - pad,
        w: box.file.right - box.file.left + 2 * pad,
        h: box.file.bottom - box.file.top + 2 * pad,
      });
    }
    return rects;
  }, [squarePx]);

  if (annotations.length === 0) return null;

  const maskIdOf = (mark: StrokeMark) =>
    still ? undefined : (segment: number) => `${id}-${strokes.indexOf(mark)}-${segment}`;
  const rings = strokes.filter((m) => m.kind === "circle");
  const arrows = strokes.filter((m) => m.kind === "arrow");

  return (
    <>
      <svg
        ref={ref}
        data-annotations
        aria-hidden="true"
        viewBox="0 0 8 8"
        className="pointer-events-none absolute inset-0 z-10 size-full overflow-visible"
      >
        <defs>
          <mask id={`${id}-coords`} maskUnits="userSpaceOnUse" x={-1} y={-1} width={10} height={10}>
            <rect x={-1} y={-1} width={10} height={10} fill="white" />
            {cuts.map((r) => (
              <rect key={`${r.x},${r.y}`} x={r.x} y={r.y} width={r.w} height={r.h} fill="black" />
            ))}
          </mask>
          {still
            ? null
            : strokes.map((mark, m) => {
                const values = tracksOf(mark);
                return mark.groups.flat().map((segment, i) => (
                  <mask
                    // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
                    key={`${mark.key}-${i}`}
                    id={`${id}-${m}-${i}`}
                    maskUnits="userSpaceOnUse"
                    x={-1}
                    y={-1}
                    width={10}
                    height={10}
                  >
                    <Reveal
                      segment={segment}
                      progress={values[segment.track] as MotionValue<number>}
                    />
                  </mask>
                ));
              })}
        </defs>
        <g mask={`url(#${id}-coords)`}>
          {rings.map((m) => (
            <MarkPaint key={`h-${m.key}`} mark={m} part="halo" maskId={maskIdOf(m)} />
          ))}
          {arrows.map((m) => (
            <MarkPaint key={m.key} mark={m} part="both" maskId={maskIdOf(m)} />
          ))}
          {rings.map((m) => (
            <MarkPaint key={`f-${m.key}`} mark={m} part="fill" maskId={maskIdOf(m)} />
          ))}
        </g>
        {badges.map((p) => (
          // Remount when `still` flips (hydration under reduced motion), so no spring is left running.
          <BadgeView key={`${p.key}:${still}`} mark={p} still={still} />
        ))}
      </svg>
      <ul className="sr-only" aria-label="Annotations on the board">
        {annotations.map((a) => (
          <li key={annotationKey(a)}>{describeAnnotation(a)}</li>
        ))}
      </ul>
    </>
  );
}
