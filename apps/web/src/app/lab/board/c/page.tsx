"use client";

import { games } from "@ci/ui/fixtures";
import { Children, cloneElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { Chessboard, defaultPieces, type PieceRenderObject } from "react-chessboard";
import { LabFrame } from "../../lab-frame";

// Variant c, "Ink well": the board pressed into a well, coordinates on the squares, and a
// variable-width pen stroke with tapered ends (DESIGN.md, "Pen stroke geometry").
// Lab only: Task 11 builds the real wrapper and packages/ui/src/annotations the real geometry.

const game = games.middlegameBlunder;
const found = game.plies.find((p) => p.judgement === "blunder");
if (!found) throw new Error("middlegameBlunder has no blunder");
const blunder = found;
const from = blunder.uci.slice(0, 2);
const to = blunder.uci.slice(2, 4);

type Pt = readonly [number, number];

/** Square centre in board units (one square = 1), White at the bottom. */
function centre(square: string): Pt {
  const file = square.charCodeAt(0) - 97;
  const rank = Number(square[1]);
  return [file + 0.5, 8 - rank + 0.5];
}

// All widths are fractions of a square: DESIGN.md's px values at a 56px square, divided by 56.
const W_START = 2.2 / 56;
const W_MID = 3.2 / 56;
const W_END = 1.4 / 56;
const HALO = 2 / 56; // each side
const HEAD_FACTOR = 0.9;

function smooth(t: number): number {
  return t * t * (3 - 2 * t);
}

/** Width profile: 2.2 at the start, 3.2 from 35% to 65%, 1.4 at the end. */
function width(t: number, scale: number): number {
  if (t < 0.35) return scale * (W_START + (W_MID - W_START) * smooth(t / 0.35));
  if (t <= 0.65) return scale * W_MID;
  return scale * (W_MID + (W_END - W_MID) * smooth((t - 0.65) / 0.35));
}

/**
 * Builds the filled outline of a variable-width stroke around a sampled centreline: the left
 * edge forward, a round cap, the right edge back and a round cap.
 */
function outline(points: Pt[], scale: number): string {
  const n = points.length;
  const left: Pt[] = [];
  const right: Pt[] = [];
  const widths: number[] = [];
  for (let i = 0; i < n; i++) {
    const p = points[i] as Pt;
    const a = points[Math.max(0, i - 1)] as Pt;
    const b = points[Math.min(n - 1, i + 1)] as Pt;
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    const len = Math.hypot(dx, dy) || 1;
    const half = width(i / (n - 1), scale) / 2;
    widths.push(half);
    left.push([p[0] - (dy / len) * half, p[1] + (dx / len) * half]);
    right.push([p[0] + (dy / len) * half, p[1] - (dx / len) * half]);
  }
  const f = (q: Pt) => `${q[0].toFixed(4)} ${q[1].toFixed(4)}`;
  const endR = (widths[n - 1] ?? 0).toFixed(4);
  const startR = (widths[0] ?? 0).toFixed(4);
  return [
    `M${f(left[0] as Pt)}`,
    ...left.slice(1).map((q) => `L${f(q)}`),
    `A${endR} ${endR} 0 0 1 ${f(right[n - 1] as Pt)}`,
    ...right
      .slice(0, -1)
      .reverse()
      .map((q) => `L${f(q)}`),
    `A${startR} ${startR} 0 0 1 ${f(left[0] as Pt)}`,
    "Z",
  ].join(" ");
}

/** A mark segment: the sheet halo first, then the fill on top (the halo rule). */
function Stroke({ d, colour }: { d: string; colour: string }) {
  return (
    <>
      <path
        d={d}
        fill="var(--color-sheet)"
        stroke="var(--color-sheet)"
        strokeWidth={HALO * 2}
        strokeLinejoin="round"
      />
      <path d={d} fill={colour} />
    </>
  );
}

function sample(f: (t: number) => Pt, steps: number): Pt[] {
  return Array.from({ length: steps + 1 }, (_, i) => f(i / steps));
}

// Ring: an ellipse spiral from -60 deg to 380 deg (seed values fixed here; the real seeding is
// FNV-1a into mulberry32 in packages/ui/src/annotations).
const RING = { ry: 0.38, ratio: 1.12, tilt: 10, phi1: 0.7, phi2: 2.1, growth: 2.5 / 56 };

function ringPoint(c: Pt, deg: number): Pt {
  const th = (deg * Math.PI) / 180;
  const drift = 1 + 0.04 * Math.sin(2 * th + RING.phi1) + 0.02 * Math.sin(3 * th + RING.phi2);
  const grow = (RING.growth * (deg + 60)) / 360;
  const ex = (RING.ry * RING.ratio * drift + grow) * Math.cos(th);
  const ey = (RING.ry * drift + grow) * Math.sin(th);
  const tilt = (RING.tilt * Math.PI) / 180;
  return [
    c[0] + ex * Math.cos(tilt) - ey * Math.sin(tilt),
    c[1] + ex * Math.sin(tilt) + ey * Math.cos(tilt),
  ];
}

function Ring({ square }: { square: string }) {
  const c = centre(square);
  // Two segments, so the overrun (360 to 380 deg) draws over the start rather than under it.
  const main = sample((t) => ringPoint(c, -60 + t * 360), 96);
  const tail = sample((t) => ringPoint(c, 300 + t * 20), 12);
  return (
    <g data-mark="ring">
      <Stroke d={outline(main, 1)} colour="var(--color-pen)" />
      <Stroke d={outline(tail, 0.85)} colour="var(--color-pen)" />
    </g>
  );
}

function Arrow({ from: a, to: b }: { from: string; to: string }) {
  const o = centre(a);
  const t = centre(b);
  const dx = t[0] - o[0];
  const dy = t[1] - o[1];
  const dist = Math.hypot(dx, dy);
  const ux = dx / dist;
  const uy = dy / dist;
  const start: Pt = [o[0] + ux * 0.22, o[1] + uy * 0.22];
  // DESIGN.md ends the shaft 0.16 short of the target centre. With a ring on the target that buries the
  // head inside the ring, so the tip stops on the ring's outer edge instead (proposal, board.md).
  const clear = 0.46;
  const end: Pt = [t[0] - ux * clear, t[1] - uy * clear];
  const len = Math.hypot(end[0] - start[0], end[1] - start[1]);
  // A 3% bow: the quadratic's control point sits at twice the wanted mid-curve offset.
  const bow = 0.03 * len * 2;
  const ctrl: Pt = [(start[0] + end[0]) / 2 - uy * bow, (start[1] + end[1]) / 2 + ux * bow];
  const shaft = sample((s) => {
    const m = 1 - s;
    return [
      m * m * start[0] + 2 * m * s * ctrl[0] + s * s * end[0],
      m * m * start[1] + 2 * m * s * ctrl[1] + s * s * end[1],
    ];
  }, 48);
  // Open head: two separate strokes, 0.9 and 1.1 of a 0.30-square base, at 26 and 32 deg.
  const back = Math.atan2(ctrl[1] - end[1], ctrl[0] - end[0]);
  const head = (angle: number, length: number): Pt[] => {
    const r = back + (angle * Math.PI) / 180;
    return sample(
      (s) => [end[0] + Math.cos(r) * length * s, end[1] + Math.sin(r) * length * s],
      12,
    );
  };
  return (
    <g data-mark="arrow">
      <Stroke d={outline(shaft, 1)} colour="var(--color-pen)" />
      <Stroke d={outline(head(26, 0.27), HEAD_FACTOR)} colour="var(--color-pen)" />
      <Stroke d={outline(head(-32, 0.33), HEAD_FACTOR)} colour="var(--color-pen)" />
    </g>
  );
}

// DESIGN.md puts the badge top right. When the arrow arrives from the right, its head lands in that
// corner, so the badge moves to the top-left corner (a proposal recorded in design/decisions/board.md).
function Badge({ square, glyph, arrowFrom }: { square: string; glyph: string; arrowFrom: string }) {
  const c = centre(square);
  const r = 0.2;
  const inset = 1 / 56;
  const side = centre(arrowFrom)[0] > c[0] ? -1 : 1;
  const cx = c[0] + side * (0.5 - r - inset);
  const cy = c[1] - 0.5 + r + inset;
  return (
    <g data-mark="badge">
      <circle cx={cx} cy={cy} r={r + HALO} fill="var(--color-sheet)" />
      <circle cx={cx} cy={cy} r={r} fill="var(--color-pen)" />
      <text
        x={cx}
        y={cy}
        textAnchor="middle"
        dominantBaseline="central"
        fill="var(--color-on-ink)"
        style={{ fontFamily: "var(--font-serif)", fontWeight: 600, fontSize: 0.28 }}
      >
        {glyph}
      </text>
    </g>
  );
}

// Cburnett paths (react-chessboard's default set, identical to the Commons files) recoloured to
// the piece tokens. Lab only: the product recolours at build time into sprites (DESIGN.md).
function recolour(node: ReactNode): ReactNode {
  if (!isValidElement(node)) return node;
  const el = node as ReactElement<{ style?: Record<string, unknown>; children?: ReactNode }>;
  const map = (v: unknown) =>
    v === "#000000" ? "var(--color-piece-dark)" : v === "#ffffff" ? "var(--color-piece-light)" : v;
  const style = el.props.style
    ? Object.fromEntries(Object.entries(el.props.style).map(([k, v]) => [k, map(v)]))
    : undefined;
  const kids = el.props.children;
  return cloneElement(el, {
    ...(style ? { style } : {}),
    ...(kids === undefined
      ? {}
      : { children: Array.isArray(kids) ? Children.toArray(kids).map(recolour) : recolour(kids) }),
  });
}

const pieces: PieceRenderObject = Object.fromEntries(
  Object.entries(defaultPieces).map(([key, render]) => [
    key,
    (props?: Parameters<typeof render>[0]) => recolour(render(props)) as ReactElement,
  ]),
);

const coord = {
  fontFamily: "var(--font-sans)",
  fontSize: "var(--text-xs)",
  lineHeight: 1,
  fontWeight: 500,
  position: "absolute" as const,
  userSelect: "none" as const,
};

export default function BoardLabC() {
  return (
    <LabFrame
      surface="board"
      variant="c"
      title="Ink well"
      rationale="The board is pressed into a well with --radius-board, so it reads as set into the sheet rather than framed. Coordinates sit on the squares (no outside lane, so the board keeps its pixels on a phone). The pen is a filled, tapered outline: thin in, full through the middle, thin out."
    >
      <main className="px-(--space-gutter) py-8">
        <div className="mx-auto w-fit rounded-(--radius-board) bg-well p-2">
          <div
            data-lab-board
            data-ply={blunder.ply}
            data-squares="well"
            data-coords="on-squares"
            data-stroke="tapered"
            className="relative overflow-hidden rounded-(--radius-board) [--lab-coord-light:var(--color-ink)] dark:[--lab-coord-light:var(--color-on-ink)]"
            style={{ width: "calc(var(--board-size) - 16px)" }}
          >
            <Chessboard
              options={{
                id: "lab-c",
                position: blunder.fenAfter,
                pieces,
                allowDragging: false,
                allowDrawingArrows: false,
                showAnimations: false,
                boardStyle: { borderRadius: "var(--radius-board)" },
                lightSquareStyle: { backgroundColor: "var(--color-board-light)" },
                darkSquareStyle: { backgroundColor: "var(--color-board-dark)" },
                lightSquareNotationStyle: { color: "var(--lab-coord-light)" },
                darkSquareNotationStyle: { color: "var(--color-ink)" },
                alphaNotationStyle: { ...coord, bottom: 2, right: 3 },
                numericNotationStyle: { ...coord, top: 3, left: 3 },
                squareStyles: {
                  [from]: { backgroundImage: "linear-gradient(var(--color-board-lastmove) 0 0)" },
                  [to]: { backgroundImage: "linear-gradient(var(--color-board-lastmove) 0 0)" },
                },
              }}
            />
            <svg
              data-lab-marks
              aria-hidden="true"
              viewBox="0 0 8 8"
              className="pointer-events-none absolute inset-0 size-full"
            >
              <Ring square={to} />
              <Arrow from={from} to={to} />
              <Badge square={to} glyph="??" arrowFrom={from} />
            </svg>
          </div>
        </div>
        <p className="mt-4 text-center font-serif text-sm text-ink-soft">
          After {Math.ceil(blunder.ply / 2)}. {blunder.san}?? (blunder)
        </p>
      </main>
    </LabFrame>
  );
}
