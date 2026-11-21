"use client";

// Lab variant C, "Coach's note": the notation is the headline at 5xl, the board is offset beside
// it, and one red-pen stroke rings the move and runs to the ring on the board. Dev only (LP). The
// board is a token-coloured placeholder until Task 11; the pen geometry is a close lab
// approximation of DESIGN.md "Pen stroke geometry" until Task 12 owns it in @ci/ui/annotations.

import { games } from "@ci/ui/fixtures";
import {
  dur,
  ease,
  motion,
  pieceWeight,
  settleFor,
  spring,
  useAnimate,
  useReducedMotion,
} from "@ci/ui/motion";
import { type CSSProperties, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { LabFrame } from "../../lab-frame";

// ---------------------------------------------------------------------------------------------
// Shared core (identical in a, b and c): fixture data, placeholder board, pen marks, the beat.
// ---------------------------------------------------------------------------------------------

/** Fixture lookups that must exist: a missing one is a broken fixture, so fail loudly. */
function must<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("styleguide hero: fixture value missing");
  return value;
}

const game = games.middlegameBlunder;
const plies = game.plies;
const drop = (i: number) => {
  const before = plies[i - 1]?.winWhite ?? 50;
  const after = plies[i]?.winWhite ?? 50;
  return game.playerColor === "white" ? before - after : after - before;
};
// The player's worst blunder of the game, by lost win chance.
const keyIndex = plies.reduce(
  (best, p, i) =>
    p.color === game.playerColor && p.judgement === "blunder" && drop(i) > drop(best) ? i : best,
  plies.findIndex((p) => p.color === game.playerColor && p.judgement === "blunder"),
);
const keyPly = must(plies[keyIndex]);
const prevPly = must(plies[keyIndex - 1]);
const nextPly = must(plies[keyIndex + 1]);

const glyphs = { blunder: "??", mistake: "?", inaccuracy: "?!" } as const;
const moveNo = (ply: number) => Math.ceil(ply / 2);
function fmtEval(cp: number | null, mate: number | null): string {
  if (mate !== null) return `${mate < 0 ? "−" : ""}M${Math.abs(mate)}`;
  const v = (cp ?? 0) / 100;
  if (v === 0) return "0.0";
  return `${v > 0 ? "+" : "−"}${Math.abs(v).toFixed(1)}`;
}
const keyNo = moveNo(keyPly.ply);
const keyGlyph = keyPly.judgement ? glyphs[keyPly.judgement] : "";
const keyEval = fmtEval(keyPly.cpWhite, keyPly.mateWhite);
const prevEval = fmtEval(prevPly.cpWhite, prevPly.mateWhite);
const from = keyPly.uci.slice(0, 2);
const to = keyPly.uci.slice(2, 4);
const prefix = keyPly.color === "white" ? `${keyNo}.` : `${keyNo}…`;

const title = `${game.white.name} v ${game.black.name}`;
const context = `Lichess ${game.timeClass}, ${game.openingName ?? "unknown opening"}. You had White.`;
// One sentence, from what the game shows: the knight blockaded e5, and Black's next move was e4.
const note = `Your knight on ${from} was holding back Black’s e-pawn; the moment it went to ${to}, the pawn came forward with ${moveNo(nextPly.ply)}…${nextPly.san}, and ${prevEval} became ${keyEval}.`;

type Row = {
  n: number;
  white?: (typeof plies)[number];
  black?: (typeof plies)[number];
};
const rows: Row[] = [];
for (let n = keyNo - 3; n <= keyNo + 4; n++) {
  rows.push({
    n,
    white: plies.find((p) => p.ply === n * 2 - 1),
    black: plies.find((p) => p.ply === n * 2),
  });
}

// Board geometry: 45 units per square, viewed from the player's side (White).
const SQ = 45;
const S = SQ / 56; // DESIGN.md pen values are given at a 56px square
type Pt = readonly [number, number];
const sqXY = (sq: string): Pt => [(sq.charCodeAt(0) - 97) * SQ, (8 - Number(sq[1])) * SQ];
const centre = (sq: string): Pt => {
  const [x, y] = sqXY(sq);
  return [x + SQ / 2, y + SQ / 2];
};

type Kind = "p" | "n" | "b" | "r" | "q" | "k";
const bodies: Record<Kind, string[]> = {
  p: [
    "M22.5 9.5a4.5 4.5 0 0 0-3.1 7.8c-2 1.2-3.2 3.3-3.2 5.6 0 1.9.8 3.6 2.2 4.8-3.3 1.9-6.4 5.6-6.4 10.8h21c0-5.2-3.1-8.9-6.4-10.8 1.4-1.2 2.2-2.9 2.2-4.8 0-2.3-1.2-4.4-3.2-5.6a4.5 4.5 0 0 0-3.1-7.8z",
  ],
  n: [
    "M23.5 9.5c-.9-1.6-2.3-2.4-3.4-2.3l.5 2.9c-4.6 1.5-8.1 5.6-9.1 10.8l-1.6 4.1c-.3 1.1.4 2 1.5 2.2l1.7.3 2.1-2.2c1.6-.3 3.6-1.2 5.3-2.8-.5 4.2-3.9 7-6.5 9.9-1.2 1.3-1.8 3-1.8 4.6h21.4c.9-9.2-1.6-19.2-10.1-27.5z",
  ],
  b: [
    "M22.5 6.5a2.3 2.3 0 1 0 0 4.6 2.3 2.3 0 0 0 0-4.6z",
    "M22.5 11.2c-4.6 3.6-7.4 8.4-6.6 13.3.3 1.7 1.2 3 2.4 3.9h8.4c1.2-.9 2.1-2.2 2.4-3.9.8-4.9-2-9.7-6.6-13.3z",
    "M17.2 28.4h10.6l1.2 3.3H16z",
    "M12.5 38.5c2.6-.4 5.6-2.3 6.4-6.8h7.2c.8 4.5 3.8 6.4 6.4 6.8z",
  ],
  r: [
    "M11.5 9.5h4.2v3.3h4.6V9.5h4.4v3.3h4.6V9.5h4.2v6.4l-3.3 2.9v11.4l3.3 2.9v5.4h-22v-5.4l3.3-2.9V18.8l-3.3-2.9z",
  ],
  q: [
    "M8.5 13.5l5.6 16.8h16.8l5.6-16.8-7.3 8.1-4.7-11-4.7 11z",
    "M14.1 30.3l-1.1 4.4c3.1 1.8 6.3 2.6 9.5 2.6s6.4-.8 9.5-2.6l-1.1-4.4z",
    "M8.5 11.2a2.2 2.2 0 1 0 0 4.4 2.2 2.2 0 0 0 0-4.4zM36.5 11.2a2.2 2.2 0 1 0 0 4.4 2.2 2.2 0 0 0 0-4.4zM22.5 7.6a2.2 2.2 0 1 0 0 4.4 2.2 2.2 0 0 0 0-4.4z",
  ],
  k: [
    "M22.5 16.5c-1.8-3.8-7.8-3.4-9.3.4-1.4 3.6.7 7.4 3.6 10l-1.3 7.6c4.6 2.6 9.4 2.6 14 0l-1.3-7.6c2.9-2.6 5-6.4 3.6-10-1.5-3.8-7.5-4.2-9.3-.4z",
  ],
};
const details: Partial<Record<Kind, string>> = {
  n: "M16.4 15.6h.1",
  r: "M14.8 18.8h15.4M14.8 30.2h15.4",
  k: "M22.5 6.5v8M19.3 9.6h6.4M17 29.5h11",
};

function Piece({ code }: { code: string }) {
  const kind = code.toLowerCase() as Kind;
  const white = code === code.toUpperCase();
  const detail = details[kind];
  return (
    <g strokeLinejoin="round" strokeLinecap="round">
      {bodies[kind].map((d) => (
        <path
          key={d}
          d={d}
          className={
            white ? "fill-piece-light stroke-piece-dark" : "fill-piece-dark stroke-piece-dark"
          }
          strokeWidth={1.5}
        />
      ))}
      {detail && (
        <path
          d={detail}
          fill="none"
          className={white ? "stroke-piece-dark" : "stroke-piece-light"}
          strokeWidth={kind === "n" ? 2.4 : 1.5}
        />
      )}
    </g>
  );
}

function parseFen(fen: string): { sq: string; code: string }[] {
  const out: { sq: string; code: string }[] = [];
  (fen.split(" ")[0] ?? "").split("/").forEach((rank, r) => {
    let f = 0;
    for (const ch of rank) {
      if (/\d/.test(ch)) f += Number(ch);
      else {
        out.push({ sq: `${String.fromCharCode(97 + f)}${8 - r}`, code: ch });
        f++;
      }
    }
  });
  return out;
}

// Variable-width pen outline around a centreline (DESIGN.md: 2.2 rising to 3.2, tapering to 1.4).
function widthAt(t: number): number {
  if (t < 0.35) return 2.2 + (t / 0.35) * 1.0;
  if (t <= 0.65) return 3.2;
  return 3.2 - ((t - 0.65) / 0.35) * 1.8;
}
const f2 = (n: number) => n.toFixed(2);
function outline(pts: Pt[], scale: number): string {
  const left: Pt[] = [];
  const right: Pt[] = [];
  const n = pts.length;
  pts.forEach((p, i) => {
    const a = must(pts[Math.max(0, i - 1)]);
    const b = must(pts[Math.min(n - 1, i + 1)]);
    const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
    const nx = -(b[1] - a[1]) / len;
    const ny = (b[0] - a[0]) / len;
    const w = Math.max(1.1, widthAt(i / (n - 1)) * scale) / 2;
    left.push([p[0] + nx * w, p[1] + ny * w]);
    right.push([p[0] - nx * w, p[1] - ny * w]);
  });
  const we = Math.max(1.1, widthAt(1) * scale) / 2;
  const ws = Math.max(1.1, widthAt(0) * scale) / 2;
  const l = left.map((p) => `${f2(p[0])} ${f2(p[1])}`).join(" L");
  const r = right
    .reverse()
    .map((p) => `${f2(p[0])} ${f2(p[1])}`)
    .join(" L");
  return `M${l} A${f2(we)} ${f2(we)} 0 0 1 ${r} A${f2(ws)} ${f2(ws)} 0 0 1 ${f2(must(left[0])[0])} ${f2(must(left[0])[1])}Z`;
}
const line = (pts: Pt[]) => `M${pts.map((p) => `${f2(p[0])} ${f2(p[1])}`).join(" L")}`;

// A tilted, drifting ellipse that spirals out past 360° to 380° (the overrun).
function ringPoints(cx: number, cy: number, rx: number, ry: number, tiltDeg: number, grow: number) {
  const tilt = (tiltDeg * Math.PI) / 180;
  const pts: Pt[] = [];
  for (let d = 0; d <= 380; d += 5) {
    const t = ((-60 - d) * Math.PI) / 180;
    const drift = 1 + 0.04 * Math.sin(2 * t + 0.7) + 0.02 * Math.sin(3 * t + 2.1);
    const g = (d / 360) * grow;
    const x = (rx * drift + g) * Math.cos(t);
    const y = (ry * drift + g) * Math.sin(t);
    pts.push([
      cx + x * Math.cos(tilt) - y * Math.sin(tilt),
      cy + x * Math.sin(tilt) + y * Math.cos(tilt),
    ]);
  }
  return { all: pts, main: pts.slice(0, 73), tail: pts.slice(72) };
}

function quad(a: Pt, c: Pt, b: Pt, steps = 24): Pt[] {
  const pts: Pt[] = [];
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    const u = 1 - t;
    pts.push([
      u * u * a[0] + 2 * u * t * c[0] + t * t * b[0],
      u * u * a[1] + 2 * u * t * c[1] + t * t * b[1],
    ]);
  }
  return pts;
}

// The pen arrow of the played move: bowed shaft, open two-stroke head with unequal strokes.
function arrowGeometry(fromSq: string, toSq: string) {
  const a = centre(fromSq);
  const b = centre(toSq);
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  const ux = (b[0] - a[0]) / len;
  const uy = (b[1] - a[1]) / len;
  const start: Pt = [a[0] + ux * 0.22 * SQ, a[1] + uy * 0.22 * SQ];
  const end: Pt = [b[0] - ux * 0.16 * SQ, b[1] - uy * 0.16 * SQ];
  const bow = 0.03 * len;
  const ctrl: Pt = [(start[0] + end[0]) / 2 - uy * bow, (start[1] + end[1]) / 2 + ux * bow];
  const shaft = quad(start, ctrl, end);
  const tx = end[0] - ctrl[0];
  const ty = end[1] - ctrl[1];
  const tl = Math.hypot(tx, ty);
  const back = Math.atan2(-ty / tl, -tx / tl);
  const head = (deg: number, k: number): Pt[] => {
    const ang = back + (deg * Math.PI) / 180;
    const L = 0.3 * SQ * k;
    return quad(
      end,
      [end[0] + Math.cos(ang) * L * 0.5, end[1] + Math.sin(ang) * L * 0.5],
      [end[0] + Math.cos(ang) * L, end[1] + Math.sin(ang) * L],
      8,
    );
  };
  return { shaft, heads: [head(26, 0.9), head(-32, 1.1)] };
}

const PEN_MAIN_EASE = [0.6, 0.05, 1, 1] as const; // the eased sweep up to 360°
const PEN_TAIL_EASE = [0, 0, 0.3, 1] as const; // the soft stop lands on the overrun
const sec = (ms: number) => ms / 1000;

/** One pen stroke: a filled outline on its sheet halo, revealed by a mask along its centreline. */
function PenStroke({
  segments,
  shape,
  timing,
  maskWidth,
  reduced,
  className = "fill-pen",
}: {
  segments: Pt[][];
  shape: string;
  timing: { delay: number; duration: number; ease: readonly number[] }[];
  maskWidth: number;
  reduced: boolean;
  className?: string;
}) {
  const id = useId().replace(/:/g, "");
  return (
    <g>
      <mask id={`m${id}`} maskUnits="userSpaceOnUse" x="-2000" y="-2000" width="6000" height="6000">
        {segments.map((seg, i) => {
          const t = timing[i] ?? must(timing[0]);
          return (
            <motion.path
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
              key={i}
              d={line(seg)}
              fill="none"
              stroke="white"
              strokeWidth={maskWidth}
              strokeLinecap="round"
              strokeLinejoin="round"
              initial={reduced ? false : { pathLength: 0, opacity: 0 }}
              animate={{ pathLength: 1, opacity: 1 }}
              transition={
                reduced
                  ? { duration: 0 }
                  : {
                      pathLength: {
                        delay: sec(t.delay),
                        duration: sec(t.duration),
                        ease: [...t.ease] as [number, number, number, number],
                      },
                      opacity: { delay: sec(t.delay), duration: 0.001 },
                    }
              }
            />
          );
        })}
      </mask>
      <g mask={`url(#m${id})`}>
        <path
          d={shape}
          className={`${className} stroke-sheet`}
          strokeWidth={4}
          paintOrder="stroke"
          strokeLinejoin="round"
        />
      </g>
    </g>
  );
}

// The one beat (DESIGN.md choreography 1, climax rows), in ms from the start of the hero.
const T = {
  select: 0,
  lift: 400,
  travel: 490,
  contact: 760,
  ring: 860,
  arrow: 980,
  heads: [1246, 1276],
  badge: 1400,
  note: 1400,
} as const;

function useBeat() {
  const reduced = useReducedMotion();
  const [run, setRun] = useState(0);
  return { reduced, key: `${reduced ? "r" : "m"}${run}`, replay: () => setRun((r) => r + 1) };
}

/** Opacity-only appearance at a beat time (or instantly under reduced motion). */
function appear(reduced: boolean, at: number) {
  return {
    initial: reduced ? false : { opacity: 0 },
    animate: { opacity: 1 },
    transition: reduced
      ? { duration: 0 }
      : {
          delay: sec(at),
          duration: dur.quick,
          ease: [...ease.out] as [number, number, number, number],
        },
  } as const;
}

const badgeText: CSSProperties = {
  fontSize: Math.max(11, 0.28 * SQ),
  letterSpacing: "var(--tracking-glyph)",
};

function Board({ reduced, className }: { reduced: boolean; className?: string }) {
  const [scope, animate] = useAnimate();
  const before = parseFen(prevPly.fenAfter).filter((p) => p.sq !== from);
  const mover = must(parseFen(prevPly.fenAfter).find((p) => p.sq === from));
  const [fx, fy] = sqXY(from);
  const [tx, ty] = sqXY(to);
  const kind = ({ p: "pawn", n: "knight", b: "bishop", r: "rook", q: "queen", k: "king" } as const)[
    mover.code.toLowerCase() as Kind
  ];
  const w = pieceWeight[kind];

  useEffect(() => {
    if (reduced) return;
    const timers: ReturnType<typeof setTimeout>[] = [];
    const at = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms));
    at(T.lift, () =>
      animate(
        "[data-lift]",
        { scale: w.lift, y: -4 },
        { duration: dur.instant, ease: [...ease.out] },
      ),
    );
    at(T.travel, () => animate("[data-travel]", { x: tx - fx, y: ty - fy }, settleFor(kind)));
    at(T.contact, async () => {
      await animate(
        "[data-lift]",
        { scale: 1, y: 0, scaleX: w.squash.scaleX, scaleY: w.squash.scaleY },
        { duration: 0 },
      );
      animate("[data-lift]", { scaleX: 1, scaleY: 1 }, spring.land);
    });
    return () => timers.forEach(clearTimeout);
  }, [reduced, animate, w, kind, fx, fy, tx, ty]);

  const ring = ringPoints(...centre(to), 1.12 * 0.38 * SQ, 0.38 * SQ, -10, 2.5 * S);
  const arrow = arrowGeometry(from, to);
  const [bx, by] = sqXY(to);
  const r = 0.2 * SQ;
  const bcx = bx + SQ - 1 - r;
  const bcy = by + 1 + r;

  return (
    <svg
      ref={scope}
      viewBox={`0 0 ${8 * SQ} ${8 * SQ}`}
      className={`block aspect-square overflow-visible ${className ?? ""}`}
      role="img"
      aria-label={`Position after ${prefix} ${keyPly.san}: a red ring on ${to} and a red arrow from ${from}, marked as a blunder.`}
    >
      {Array.from({ length: 64 }, (_, i) => {
        const f = i % 8;
        const rk = Math.floor(i / 8);
        return (
          <rect
            // biome-ignore lint/suspicious/noArrayIndexKey: the 64 squares never reorder
            key={i}
            x={f * SQ}
            y={rk * SQ}
            width={SQ}
            height={SQ}
            className={(f + rk) % 2 === 0 ? "fill-board-light" : "fill-board-dark"}
          />
        );
      })}
      {[from, to].map((sq) => {
        const [x, y] = sqXY(sq);
        return (
          <motion.rect
            key={sq}
            x={x}
            y={y}
            width={SQ}
            height={SQ}
            className="fill-board-lastmove"
            {...appear(reduced, T.contact)}
          />
        );
      })}
      {before.map((p) => {
        const [x, y] = sqXY(p.sq);
        return (
          <g key={p.sq} transform={`translate(${x} ${y})`}>
            <Piece code={p.code} />
          </g>
        );
      })}
      <g transform={`translate(${fx} ${fy})`}>
        <motion.g data-travel style={reduced ? { x: tx - fx, y: ty - fy } : undefined}>
          <motion.g data-lift style={{ originX: 0.5, originY: 0.9 }}>
            <Piece code={mover.code} />
          </motion.g>
        </motion.g>
      </g>
      <PenStroke
        reduced={reduced}
        segments={[ring.main, ring.tail]}
        shape={outline(ring.all, S)}
        maskWidth={3.2 * S + 5}
        timing={[
          { delay: T.ring, duration: 318, ease: PEN_MAIN_EASE },
          { delay: T.ring + 318, duration: 102, ease: PEN_TAIL_EASE },
        ]}
      />
      <PenStroke
        reduced={reduced}
        segments={[arrow.shaft]}
        shape={outline(arrow.shaft, S)}
        maskWidth={3.2 * S + 5}
        timing={[{ delay: T.arrow, duration: 420, ease: ease.pen }]}
      />
      {arrow.heads.map((h, i) => (
        <PenStroke
          // biome-ignore lint/suspicious/noArrayIndexKey: two fixed head strokes
          key={i}
          reduced={reduced}
          segments={[h]}
          shape={outline(h, S * 0.9)}
          maskWidth={3 * S + 5}
          timing={[{ delay: T.heads[i] ?? T.heads[0], duration: 90, ease: ease.out }]}
        />
      ))}
      <motion.g
        style={{ originX: `${bcx}px`, originY: `${bcy}px` }}
        initial={reduced ? false : { scale: 0, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={
          reduced
            ? { duration: 0 }
            : {
                delay: sec(T.badge),
                ...spring.land,
                opacity: { delay: sec(T.badge), duration: 0.05 },
              }
        }
      >
        <circle
          cx={bcx}
          cy={bcy}
          r={r}
          className="fill-pen stroke-sheet"
          strokeWidth={2}
          paintOrder="stroke"
        />
        <text
          x={bcx}
          y={bcy}
          textAnchor="middle"
          dominantBaseline="central"
          className="fill-on-ink font-serif font-semibold"
          style={badgeText}
        >
          {keyGlyph}
        </text>
      </motion.g>
    </svg>
  );
}

function Replay({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      data-lab-chrome
      onClick={onClick}
      className="min-h-11 rounded-field px-3 font-sans text-sm font-medium text-ink-soft hover:bg-sheet"
    >
      Play the moment again
    </button>
  );
}

// ---------------------------------------------------------------------------------------------
// Variant C layout
// ---------------------------------------------------------------------------------------------

const LINK_AT = 1400; // the linking stroke draws as the badge lands, over --dur-draw
const NOTE_AT = LINK_AT + 420;

function cubic(a: Pt, b: Pt, c: Pt, d: Pt, steps = 48): Pt[] {
  const pts: Pt[] = [];
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    const u = 1 - t;
    pts.push([
      u * u * u * a[0] + 3 * u * u * t * b[0] + 3 * u * t * t * c[0] + t * t * t * d[0],
      u * u * u * a[1] + 3 * u * u * t * b[1] + 3 * u * t * t * c[1] + t * t * t * d[1],
    ]);
  }
  return pts;
}

type Box = { x: number; y: number; w: number; h: number };
const boxOf = (el: Element | null, root: Element | null): Box | null => {
  if (!el || !root) return null;
  const a = el.getBoundingClientRect();
  const r = root.getBoundingClientRect();
  return { x: a.left - r.left, y: a.top - r.top, w: a.width, h: a.height };
};

/** The headline ring and the stroke that runs from it to the ring on the board. */
function PenLink({
  reduced,
  word,
  after,
  board,
}: {
  reduced: boolean;
  word: Box;
  after: Box;
  board: Box;
}) {
  const cx = word.x + word.w / 2;
  const cy = word.y + word.h / 2;
  const rx = word.w / 2 + 14;
  const ry = word.h / 2 - 2;
  const ring = ringPoints(cx, cy, rx, ry, -3, 4);
  const sq = board.w / 8;
  const tf = to.charCodeAt(0) - 97;
  const tr = Number(to[1]);
  const tx = board.x + (tf + 0.5) * sq;
  const ty = board.y + (8 - tr + 0.5) * sq;
  // Route the stroke around the text, never across it: beside the board it drops through the
  // gutter and enters along the target's rank; under the board it rises along the target's file.
  const below = board.y + board.h <= word.y;
  let link: Pt[];
  if (below) {
    const start: Pt = [cx + rx * 0.35, cy - ry - 6];
    const end: Pt = [tx, ty + sq * 0.55];
    link = cubic(start, [start[0], start[1] - 40], [end[0], end[1] + 60], end);
  } else {
    // Over the evaluation, then down the gutter and in along the target's rank.
    const start: Pt = [cx + rx * 0.55, cy - ry - 4];
    const over: Pt = [Math.min(board.x - 40, after.x + after.w + 28), after.y + after.h * 0.55];
    const gx = board.x - 28;
    const end: Pt = [tx - sq * 0.55, ty];
    link = [
      ...cubic(start, [start[0] + 40, start[1] - 46], [over[0], after.y - 46], over, 32),
      ...cubic(
        over,
        [over[0] + 8, over[1] + 140],
        [gx, ty - sq * 2.2],
        [gx, ty - sq * 0.7],
        32,
      ).slice(1),
      ...cubic([gx, ty - sq * 0.7], [gx, ty - sq * 0.1], [gx + sq * 0.2, ty], end, 16).slice(1),
    ];
  }
  const s = 1.4;
  return (
    <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden>
      <PenStroke
        reduced={reduced}
        segments={[ring.main, ring.tail]}
        shape={outline(ring.all, s)}
        maskWidth={3.2 * s + 5}
        timing={[
          { delay: T.ring, duration: 318, ease: PEN_MAIN_EASE },
          { delay: T.ring + 318, duration: 102, ease: PEN_TAIL_EASE },
        ]}
      />
      <PenStroke
        reduced={reduced}
        segments={[link]}
        shape={outline(link, 0.9)}
        maskWidth={3.2 * 0.9 + 5}
        timing={[{ delay: LINK_AT, duration: 420, ease: ease.pen }]}
      />
    </svg>
  );
}

function Hero({ reduced }: { reduced: boolean }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const wordRef = useRef<HTMLSpanElement>(null);
  const evalRef = useRef<HTMLSpanElement>(null);
  const boardRef = useRef<HTMLDivElement>(null);
  const [boxes, setBoxes] = useState<{ word: Box; after: Box; board: Box } | null>(null);
  useLayoutEffect(() => {
    const measure = () => {
      const word = boxOf(wordRef.current, rootRef.current);
      const after = boxOf(evalRef.current, rootRef.current);
      const board = boxOf(boardRef.current, rootRef.current);
      setBoxes(word && after && board ? { word, after, board } : null);
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (rootRef.current) ro.observe(rootRef.current);
    void document.fonts?.ready.then(measure);
    return () => ro.disconnect();
  }, []);

  return (
    <div className="mx-auto max-w-7xl px-(--space-gutter) pt-6 pb-16 md:pt-10">
      <div
        ref={rootRef}
        className="relative flex flex-col gap-8 md:grid md:grid-cols-[minmax(0,1fr)_auto] md:gap-x-16 lg:gap-x-24"
      >
        <div className="md:pt-4">
          <h1 className="font-serif text-5xl font-semibold tabular-nums lining-nums">
            <span className="font-normal text-ink-faint">{prefix} </span>
            <span ref={wordRef} className="mx-3 inline-block">
              {keyPly.san}
              <motion.span className="tracking-glyph text-pen-text" {...appear(reduced, T.ring)}>
                {keyGlyph}
              </motion.span>
            </span>{" "}
            <motion.span
              ref={evalRef}
              className="inline-block font-normal text-ink-faint"
              {...appear(reduced, T.ring)}
            >
              {keyEval}
            </motion.span>
          </h1>
          <motion.p
            className="mt-8 max-w-[34ch] font-serif-italic text-2xl text-ink italic"
            {...appear(reduced, NOTE_AT)}
          >
            {note}
          </motion.p>
          <p className="mt-6 max-w-[60ch] text-ink-soft">
            {title}. {context}
          </p>
        </div>

        <div ref={boardRef} className="order-first md:order-none md:mt-24">
          <Board reduced={reduced} className="w-(--board-size) rounded-board" />
        </div>

        {boxes && <PenLink reduced={reduced} {...boxes} />}
      </div>
    </div>
  );
}

export default function StyleguideHeroC() {
  const beat = useBeat();
  return (
    <LabFrame
      surface="styleguide-hero"
      variant="c"
      title="Coach's note"
      rationale="The move is the headline at 5xl; the board is offset beside it; one red-pen stroke rings the move and runs to the ring on the board."
    >
      <div className="mx-auto flex max-w-7xl justify-end px-(--space-gutter)">
        <Replay onClick={beat.replay} />
      </div>
      <Hero key={beat.key} reduced={beat.reduced} />
    </LabFrame>
  );
}
