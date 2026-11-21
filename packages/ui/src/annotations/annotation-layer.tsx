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
  HEAD_SCALE,
  isKnightMove,
  knightCorner,
  maxWidth,
  outlinePoints,
  polyline,
  ringPoints,
  samplePath,
  squareCenter,
  TONE_SCALE,
  timeAtProgress,
} from "./geometry.js";
import { parsePath } from "./path.js";
import type { Annotation, AnnotationLayerProps, Orientation, Point, Tone } from "./types.js";

// The red-pen overlay (DESIGN.md, "Severity and marks", "Pen stroke geometry", M2, M3, M10).
// Every stroke is a filled, tapered outline, painted twice (the `sheet` halo, then the fill) and
// revealed through a mask whose centreline draws on by `pathLength`.

const TONE_COLOUR: Readonly<Record<Tone, string>> = {
  pen: "var(--color-pen)",
  good: "var(--color-good)",
  pencil: "var(--color-pencil)",
};

const TONE_WORD: Readonly<Record<Tone, string>> = { pen: "Red", good: "Green", pencil: "Grey" };

/** What assistive tech hears for a mark: "Red arrow from e4 to e6", "Green circle on f7". */
export function describeAnnotation(a: Annotation): string {
  const tone = TONE_WORD[a.tone];
  if (a.kind === "arrow") return `${tone} arrow from ${a.from} to ${a.to}`;
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
  key: string;
  tone: Tone;
  /** each group paints all its halos, then all its fills, so a group reads as one stroke */
  groups: Segment[][];
  tracks: Track[];
}

interface BadgeMark {
  kind: "mark";
  key: string;
  tone: Tone;
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

function planMarks(
  annotations: readonly Annotation[],
  orientation: Orientation,
  squarePx: number,
): Plan[] {
  const px = 1 / squarePx;
  const minWidth = 1.1 * px;
  const pad = 2 * HALO + px;
  const ringed = new Set(annotations.flatMap((a) => (a.kind === "circle" ? [a.square] : [])));
  const arrows = annotations.filter(
    (a): a is Extract<Annotation, { kind: "arrow" }> => a.kind === "arrow",
  );
  const plans: Plan[] = [];
  /** For each target, the x each arrow arrives from (its last leg), to place the badge. */
  const arrivals = new Map<string, number[]>();
  let strokeIndex = 0;
  let lastEnd = 0;

  for (const a of annotations) {
    const scale = TONE_SCALE[a.tone];
    if (a.kind === "arrow") {
      const delay = strokeIndex++ * stagger.pen;
      // A knight's L runs its long leg first, unless that corner sits on another arrow's end.
      let knight: "long" | "short" = "long";
      if (isKnightMove(a.from, a.to)) {
        const c = knightCorner(a.from, a.to, orientation, "long");
        const sq = cellToSquare(Math.floor(c.x), Math.floor(c.y), orientation);
        if (arrows.some((o) => o !== a && (o.from === sq || o.to === sq))) knight = "short";
      }
      const leg = isKnightMove(a.from, a.to)
        ? knightCorner(a.from, a.to, orientation, knight)
        : squareCenter(a.from, orientation);
      arrivals.set(a.to, [...(arrivals.get(a.to) ?? []), leg.x]);
      const { shaft, head } = arrowGeometry(a.from, a.to, orientation, {
        ringed: ringed.has(a.to),
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
      plans.push({
        kind: "arrow",
        key: `arrow:${a.from}${a.to}:${a.tone}`,
        tone: a.tone,
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
        // One group: the overrun inks over the start, as a pen does, with no halo cut between.
        groups: [segs],
        tracks: [{ delay, duration: dur.draw }],
      });
    }
  }

  // Badges land when the last stroke ends, not by stagger (DESIGN.md, "Stagger scale").
  for (const a of annotations) {
    if (a.kind !== "mark") continue;
    // Top right, unless any arrow arrives on its last leg from the right of the target as drawn:
    // then top left, so the badge never covers an arrowhead (ruling 2, Task 10, by leg).
    const { col, row } = squareToCell(a.square, orientation);
    const fromRight = (arrivals.get(a.square) ?? []).some((x) => x > col + 0.5 + 1e-6);
    const r = BADGE.r;
    const inset = px;
    const left = fromRight;
    // The left column carries the rank numbers top left: a badge there drops to the bottom-left
    // corner, so it never covers a coordinate.
    const bottom = left && col === 0;
    const cx = left ? col + inset + r : col + 1 - inset - r;
    const cy = bottom ? row + 1 - inset - r : row + inset + r;
    const font = Math.max(a.glyph.length === 1 ? BADGE.font1 : BADGE.font2, BADGE.minFontPx * px);
    plans.push({
      kind: "mark",
      key: `mark:${a.square}:${a.glyph}:${a.tone}`,
      tone: a.tone,
      glyph: a.glyph,
      cx,
      cy,
      r,
      fontSize: font,
      at: lastEnd,
    });
  }
  return plans;
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

function StrokeMarkView({
  mark,
  maskBase,
  still,
}: {
  mark: StrokeMark;
  maskBase: string;
  still: boolean;
}) {
  // One progress value per track; a ring's three pieces share one, split by arc (M2). The key
  // names the mark's kind, so its number of tracks never changes.
  const [tracks] = useState(() => mark.tracks.map(() => motionValue(still ? 1 : 0)));
  // The timing never depends on the square size, so a resize re-plans without replaying.
  const timing = useRef(mark.tracks);
  timing.current = mark.tracks;
  useEffect(() => {
    if (still) {
      for (const t of tracks) t.set(1);
      return;
    }
    const controls = tracks.map((t, i) => {
      const track = timing.current[i] as Track;
      t.set(0);
      return animate(t, 1, { delay: track.delay, duration: track.duration, ease: [...ease.pen] });
    });
    return () => {
      for (const c of controls) c.stop();
    };
  }, [still, tracks]);

  const colour = TONE_COLOUR[mark.tone];
  const segments = mark.groups.flat();
  const maskId = (i: number) => `${maskBase}-${i}`;
  let index = 0;
  return (
    <g data-mark={mark.kind} data-tone={mark.tone}>
      <defs>
        {segments.map((s, i) => (
          <mask
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
            key={i}
            id={maskId(i)}
            maskUnits="userSpaceOnUse"
            x={-1}
            y={-1}
            width={10}
            height={10}
          >
            <Reveal segment={s} progress={tracks[s.track] as MotionValue<number>} />
          </mask>
        ))}
      </defs>
      {mark.groups.map((group, g) => {
        const first = index;
        index += group.length;
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed group order
          <g key={g}>
            {group.map((s, i) => (
              <path
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
                key={`h${i}`}
                data-part="halo"
                d={s.outline}
                mask={`url(#${maskId(first + i)})`}
                fill="var(--color-sheet)"
                stroke="var(--color-sheet)"
                strokeWidth={HALO * 2}
                strokeLinejoin="round"
              />
            ))}
            {group.map((s, i) => (
              <path
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
                key={`f${i}`}
                data-part="fill"
                d={s.outline}
                mask={`url(#${maskId(first + i)})`}
                fill={colour}
              />
            ))}
          </g>
        );
      })}
    </g>
  );
}

function BadgeView({ mark, still }: { mark: BadgeMark; still: boolean }) {
  const colour = TONE_COLOUR[mark.tone];
  return (
    <motion.g
      data-mark="badge"
      data-tone={mark.tone}
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
      <circle cx={mark.cx} cy={mark.cy} r={mark.r} fill={colour} />
      <text
        x={mark.cx}
        y={mark.cy}
        textAnchor="middle"
        dominantBaseline="central"
        fill="var(--color-on-ink)"
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

/**
 * The annotation overlay: an SVG in board units laid over the board, and a visually hidden list
 * that says what each mark is. Strokes draw on in order, `stagger.pen` apart, each over `dur.draw`
 * with `ease.pen`; arrowheads start at 85% of their shaft; badges land on `land` when the last
 * stroke ends. With `animate={false}` or reduced motion, everything is drawn at once.
 */
export function AnnotationLayer({
  annotations,
  orientation,
  animate = true,
}: AnnotationLayerProps) {
  const reduced = useReducedMotion();
  const still = !animate || reduced;
  const ref = useRef<SVGSVGElement>(null);
  const squarePx = useSquarePx(ref);
  const id = `ann-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const plans = useMemo(
    () => planMarks(annotations, orientation, squarePx),
    [annotations, orientation, squarePx],
  );

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
        </defs>
        <g mask={`url(#${id}-coords)`}>
          {plans.map((p, i) =>
            p.kind === "mark" ? null : (
              <StrokeMarkView key={p.key} mark={p} maskBase={`${id}-${i}`} still={still} />
            ),
          )}
        </g>
        {plans.map((p) =>
          p.kind === "mark" ? (
            // Remount when `still` flips (hydration under reduced motion), so no spring is left running.
            <BadgeView key={`${p.key}:${still}`} mark={p} still={still} />
          ) : null,
        )}
      </svg>
      <ul className="sr-only">
        {annotations.map((a) => (
          <li key={describeAnnotation(a)}>{describeAnnotation(a)}</li>
        ))}
      </ul>
    </>
  );
}
