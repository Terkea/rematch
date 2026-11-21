"use client";

// Lab variant A, "Scoresheet margin": the board heads a ruled scoresheet, the red pen rings the
// move on its row and on its square, one link stroke ties them, and the comment sits in the
// margin. Dev only (LP).
// The board is a token-coloured placeholder until Task 11; the pen geometry is a close lab
// approximation of DESIGN.md "Pen stroke geometry" until Task 12 owns it in @ci/ui/annotations.

import { games } from "@ci/ui/fixtures";
import {
  animate,
  type CubicBezier,
  dur,
  ease,
  motion,
  pieceWeight,
  settleFor,
  spring,
  stagger,
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

// The pen arrow of the played move: bowed shaft, open two-stroke head with unequal strokes. A
// ringed target ends the shaft 46% short, so the head meets the ring's outer edge (Task 10).
function arrowGeometry(fromSq: string, toSq: string, ringed: boolean) {
  const a = centre(fromSq);
  const b = centre(toSq);
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  const ux = (b[0] - a[0]) / len;
  const uy = (b[1] - a[1]) / len;
  const start: Pt = [a[0] + ux * 0.22 * SQ, a[1] + uy * 0.22 * SQ];
  const short = ringed ? 0.46 : 0.16;
  const end: Pt = [b[0] - ux * short * SQ, b[1] - uy * short * SQ];
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

const sec = (ms: number) => ms / 1000;
const ms = (s: number) => Math.round(s * 1000);

/** The real-time fraction at which a cubic-bezier ease reaches `progress` (DESIGN.md M2, M3). */
function timeAtProgress(e: CubicBezier, progress: number): number {
  const [x1, y1, x2, y2] = e;
  const at = (a: number, b: number, s: number) =>
    3 * (1 - s) * (1 - s) * s * a + 3 * (1 - s) * s * s * b + s * s * s;
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    if (at(y1, y2, mid) < progress) lo = mid;
    else hi = mid;
  }
  return at(x1, x2, (lo + hi) / 2);
}

const polyLength = (pts: Pt[]) =>
  pts.reduce((sum, p, i) => {
    const q = pts[i - 1];
    return q ? sum + Math.hypot(p[0] - q[0], p[1] - q[1]) : sum;
  }, 0);

/**
 * One pen stroke: a filled outline on its sheet halo, revealed by a mask along its centreline.
 * One `--ease-pen` progress drives every mask segment, split by arc length, so a ring's overrun
 * draws after its first 360° and the soft stop lands on the overrun (M2).
 */
function PenStroke({
  segments,
  shape,
  at,
  duration,
  maskWidth,
  reduced,
  className = "fill-pen",
  halo = true,
}: {
  segments: Pt[][];
  shape: string;
  /** Start, in ms on the beat. */
  at: number;
  /** A duration token, in seconds. */
  duration: number;
  maskWidth: number;
  reduced: boolean;
  className?: string;
  /** The 2px `sheet` halo (the halo rule). Off only where the mark already holds 3:1 alone. */
  halo?: boolean;
}) {
  const id = useId().replace(/:/g, "");
  const paths = useRef<(SVGPathElement | null)[]>([]);
  const lengths = segments.map(polyLength);
  const total = lengths.reduce((a, b) => a + b, 0) || 1;
  let acc = 0;
  const bounds = lengths.map((l) => {
    const lo = acc / total;
    acc += l;
    return [lo, acc / total] as const;
  });
  const boundsRef = useRef(bounds);
  boundsRef.current = bounds;

  useEffect(() => {
    if (reduced) return;
    const paint = (p: number) => {
      paths.current.forEach((el, i) => {
        const [lo, hi] = boundsRef.current[i] ?? [0, 1];
        if (!el) return;
        const local = Math.min(1, Math.max(0, (p - lo) / (hi - lo || 1)));
        el.style.strokeDashoffset = String(1 - local);
        el.style.opacity = p > lo ? "1" : "0";
      });
    };
    paint(0);
    const controls = animate(0, 1, {
      delay: sec(at),
      duration,
      ease: [...ease.pen],
      onUpdate: paint,
    });
    return () => controls.stop();
  }, [reduced, at, duration]);

  return (
    <g>
      <mask id={`m${id}`} maskUnits="userSpaceOnUse" x="-2000" y="-2000" width="6000" height="6000">
        {segments.map((seg, i) => (
          <path
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed segment order
            key={i}
            ref={(el) => {
              paths.current[i] = el;
            }}
            d={line(seg)}
            fill="none"
            stroke="white"
            strokeWidth={maskWidth}
            strokeLinecap="round"
            strokeLinejoin="round"
            pathLength={1}
            strokeDasharray="1 2"
            strokeDashoffset={reduced ? 0 : 1}
            opacity={reduced ? 1 : 0}
          />
        ))}
      </mask>
      <g mask={`url(#m${id})`}>
        <path
          d={shape}
          className={`${className} stroke-sheet`}
          strokeWidth={halo ? 4 : 0}
          paintOrder="stroke"
          strokeLinejoin="round"
        />
      </g>
    </g>
  );
}

// The one beat: DESIGN.md choreography 1, climax rows, in ms from the start of the hero. The fixed
// times (lift at 400, contact at 760, ring 100ms after contact, the 30ms head gap of M3) come from
// that table; everything else follows from the motion tokens.
const LIFT = 400;
const CONTACT = 760;
const RING = CONTACT + 100;
const ARROW = RING + ms(stagger.pen);
const HEAD = ARROW + ms(dur.draw * timeAtProgress(ease.pen, 0.85)); // M3: at 85% of the shaft
const BADGE = ARROW + ms(dur.draw); // the last stroke's soft stop
const T = {
  select: 0,
  lift: LIFT,
  travel: LIFT + ms(dur.instant),
  contact: CONTACT,
  ring: RING,
  arrow: ARROW,
  heads: [HEAD, HEAD + 30],
  badge: BADGE,
  link: BADGE, // the link mark draws after the badge lands
  note: BADGE + ms(dur.draw), // the comment follows the evidence
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

const pieceKinds = {
  p: "pawn",
  n: "knight",
  b: "bishop",
  r: "rook",
  q: "queen",
  k: "king",
} as const;

// Knights arc (DESIGN.md "Physical feel", Task 6 fix round 2): a knight jumps. Over its travel its
// lift grows by these amounts at the midpoint, along sin(π × travel progress); other pieces slide.
const KNIGHT_ARC = { scale: 0.08, rise: 0.2 } as const; // rise in squares

function Board({ reduced, className }: { reduced: boolean; className?: string }) {
  const travelRef = useRef<SVGGElement>(null);
  const liftRef = useRef<SVGGElement>(null);
  const before = parseFen(prevPly.fenAfter).filter((p) => p.sq !== from);
  const mover = must(parseFen(prevPly.fenAfter).find((p) => p.sq === from));
  const [fx, fy] = sqXY(from);
  const [tx, ty] = sqXY(to);
  const dx = tx - fx;
  const dy = ty - fy;
  const kind = pieceKinds[mover.code.toLowerCase() as Kind];
  const w = pieceWeight[kind];
  const arc = kind === "knight";

  // Lift, travel and land (DESIGN.md "Physical feel"), driven imperatively so the knight's arc can
  // follow the travel spring's own progress. Transform only (M9).
  useEffect(() => {
    if (reduced) return;
    const travel = travelRef.current;
    const lift = liftRef.current;
    if (!travel || !lift) return;
    const pose = { x: 0, y: 0, s: 1, sx: 1, sy: 1, ly: 0 };
    const paint = () => {
      travel.style.transform = `translate(${pose.x}px, ${pose.y}px)`;
      lift.style.transform = `translateY(${pose.ly}px) scale(${pose.s * pose.sx}, ${pose.s * pose.sy})`;
    };
    paint();
    const running: { stop: () => void }[] = [];
    const timers: ReturnType<typeof setTimeout>[] = [];
    const at = (msAt: number, fn: () => void) => timers.push(setTimeout(fn, msAt));
    at(T.lift, () =>
      running.push(
        animate(0, 1, {
          duration: dur.instant,
          ease: [...ease.out],
          onUpdate: (p) => {
            pose.s = 1 + (w.lift - 1) * p;
            pose.ly = -4 * p;
            paint();
          },
        }),
      ),
    );
    at(T.travel, () =>
      running.push(
        animate(0, 1, {
          ...settleFor(kind),
          onUpdate: (p) => {
            const hop = arc ? Math.sin(Math.PI * Math.min(1, Math.max(0, p))) : 0;
            pose.x = dx * p;
            pose.y = dy * p;
            pose.s = w.lift + KNIGHT_ARC.scale * hop;
            pose.ly = -4 - KNIGHT_ARC.rise * SQ * hop;
            paint();
          },
        }),
      ),
    );
    at(T.contact, () => {
      for (const r of running) r.stop();
      Object.assign(pose, { x: dx, y: dy, s: 1, ly: 0, sx: w.squash.scaleX, sy: w.squash.scaleY });
      paint();
      running.push(
        animate(0, 1, {
          ...spring.land,
          onUpdate: (p) => {
            pose.sx = w.squash.scaleX + (1 - w.squash.scaleX) * p;
            pose.sy = w.squash.scaleY + (1 - w.squash.scaleY) * p;
            paint();
          },
        }),
      );
    });
    return () => {
      for (const t of timers) clearTimeout(t);
      for (const r of running) r.stop();
    };
  }, [reduced, w, kind, arc, dx, dy]);

  const ring = ringPoints(...centre(to), 1.12 * 0.38 * SQ, 0.38 * SQ, -10, 2.5 * S);
  const arrow = arrowGeometry(from, to, true);
  const [bx, by] = sqXY(to);
  const r = 0.2 * SQ;
  // The badge moves to the top-left corner when the arrow arrives from the right (Task 10 ruling).
  const fromRight = centre(from)[0] > centre(to)[0];
  const bcx = fromRight ? bx + 1 + r : bx + SQ - 1 - r;
  const bcy = by + 1 + r;

  return (
    <svg
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
        <g
          ref={travelRef}
          style={reduced ? { transform: `translate(${dx}px, ${dy}px)` } : undefined}
        >
          <g ref={liftRef} style={{ transformBox: "fill-box", transformOrigin: "50% 90%" }}>
            <Piece code={mover.code} />
          </g>
        </g>
      </g>
      <PenStroke
        reduced={reduced}
        segments={[ring.main, ring.tail]}
        shape={outline(ring.all, S)}
        maskWidth={3.2 * S + 5}
        at={T.ring}
        duration={dur.draw}
      />
      <PenStroke
        reduced={reduced}
        segments={[arrow.shaft]}
        shape={outline(arrow.shaft, S)}
        maskWidth={3.2 * S + 5}
        at={T.arrow}
        duration={dur.draw}
      />
      {arrow.heads.map((h, i) => (
        <PenStroke
          // biome-ignore lint/suspicious/noArrayIndexKey: two fixed head strokes
          key={i}
          reduced={reduced}
          segments={[h]}
          shape={outline(h, S * 0.9)}
          maskWidth={3 * S + 5}
          at={T.heads[i] ?? T.heads[0]}
          duration={dur.instant}
        />
      ))}
      <motion.g
        style={{ originX: 0.5, originY: 0.5 }}
        initial={reduced ? false : { scale: 0, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={
          reduced
            ? { duration: 0 }
            : {
                delay: sec(T.badge),
                ...spring.land,
                opacity: { delay: sec(T.badge), duration: 0 },
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

/** A move in notation: Literata 600, tabular lining figures, glyph tight in pen or plain ink. */
function San({ ply, showEval = true }: { ply?: (typeof plies)[number]; showEval?: boolean }) {
  if (!ply) return null;
  const g = ply.judgement ? glyphs[ply.judgement] : "";
  return (
    <>
      {ply.san}
      {g && <span className="tracking-glyph text-pen-text">{g}</span>}
      {g && showEval && (
        <span className="font-normal text-ink-faint"> {fmtEval(ply.cpWhite, ply.mateWhite)}</span>
      )}
    </>
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
// Variant A layout
// ---------------------------------------------------------------------------------------------

// The scoresheet opens on the ringed move, so the link rises from its ring straight into the board.
// It ends before the next blunder, so no second `??` competes with the key move.
const sheetRows: Row[] = [];
for (const row of rows.filter((r) => r.n >= keyNo)) {
  const other = [row.white, row.black].some(
    (p) => p && p.ply !== keyPly.ply && p.judgement === "blunder",
  );
  if (other) break;
  sheetRows.push(row);
}
const toMove = keyPly.color === "white" ? "Black" : "White";

// Win chance on Lichess's scale (DESIGN.md "Eval graph"), from White's point of view.
const winChance = (p: { cpWhite: number | null; mateWhite: number | null }) => {
  if (p.mateWhite !== null) return p.mateWhite > 0 ? 1 : -1;
  const wc = 2 / (1 + Math.exp(-0.00368208 * (p.cpWhite ?? 0))) - 1;
  return Math.min(1, Math.max(-1, wc));
};
const wcBefore = winChance(prevPly);
const wcAfter = winChance(keyPly);
// White's share of the eval bar, before and after the move. A change of 0.3 or more swings big.
const shareBefore = (wcBefore + 1) / 2;
const shareAfter = (wcAfter + 1) / 2;
const barSpring = Math.abs(wcAfter - wcBefore) >= 0.3 ? spring.swingBig : spring.swing;

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

/** Tracks an element's box. */
function useMeasure<E extends HTMLElement>() {
  const ref = useRef<E>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setSize({ width: el.offsetWidth, height: el.offsetHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size] as const;
}

/** Runs `fn` once at a beat time, unless motion is reduced. */
function useAt(reduced: boolean, atMs: number, fn: () => (() => void) | undefined) {
  const fnRef = useRef(fn);
  fnRef.current = fn;
  useEffect(() => {
    if (reduced) return;
    let stop: (() => void) | undefined;
    const timer = setTimeout(() => {
      stop = fnRef.current();
    }, atMs);
    return () => {
      clearTimeout(timer);
      stop?.();
    };
  }, [reduced, atMs]);
}

/**
 * The eval bar (DESIGN.md "Eval bar"): 16px and vertical beside the board from 768px, 8px and
 * horizontal above it on a phone. At contact it swings to the new value, on `swing-big` when the
 * win chance moves by 0.3 or more, so it visibly overshoots and comes back.
 */
function EvalBar({ reduced, vertical }: { reduced: boolean; vertical: boolean }) {
  const whiteRef = useRef<HTMLDivElement>(null);
  useAt(reduced, T.contact, () => {
    const el = whiteRef.current;
    if (!el) return undefined;
    const run = animate(
      el,
      vertical ? { scaleY: [shareBefore, shareAfter] } : { scaleX: [shareBefore, shareAfter] },
      barSpring,
    );
    return () => run.stop();
  });
  const start = reduced ? shareAfter : shareBefore;
  const label = (value: string, show: "before" | "after", whiteWins: boolean) => {
    const motionProps =
      show === "after"
        ? appear(reduced, T.contact)
        : {
            initial: { opacity: reduced ? 0 : 1 },
            animate: { opacity: 0 },
            transition: { delay: reduced ? 0 : sec(T.contact), duration: 0 },
          };
    if (vertical) {
      // Inside the bar at the winning side's end, in the losing side's fill colour.
      return (
        <motion.span
          className={`absolute left-0 w-full rotate-180 text-center font-serif text-2xs font-semibold tabular-nums lining-nums [writing-mode:vertical-rl] ${
            whiteWins ? "bottom-1 text-ink dark:text-well" : "top-1 text-sheet dark:text-ink"
          }`}
          {...motionProps}
        >
          {value}
        </motion.span>
      );
    }
    return (
      <motion.span
        className="absolute top-1/2 right-0 -translate-y-1/2 font-serif text-2xs font-semibold tabular-nums lining-nums"
        {...motionProps}
      >
        {value}
      </motion.span>
    );
  };
  return (
    <div
      className={vertical ? "relative h-(--board-size) w-4 shrink-0" : "relative h-3.5 w-full pr-9"}
      role="img"
      aria-label={`Evaluation ${keyEval} after ${prefix} ${keyPly.san}, ${prevEval} before it.`}
    >
      <div
        className={`relative overflow-hidden rounded-board border border-rule-strong bg-ink dark:bg-well ${
          vertical ? "size-full" : "top-0.75 h-2 w-full"
        }`}
      >
        <div
          ref={whiteRef}
          className={`absolute inset-0 bg-sheet dark:bg-ink ${vertical ? "origin-bottom" : "origin-left"}`}
          style={{ transform: vertical ? `scaleY(${start})` : `scaleX(${start})` }}
        />
        {vertical && label(prevEval, "before", wcBefore >= 0)}
        {vertical && label(keyEval, "after", wcAfter >= 0)}
      </div>
      {!vertical && label(prevEval, "before", wcBefore >= 0)}
      {!vertical && label(keyEval, "after", wcAfter >= 0)}
    </div>
  );
}

/**
 * The eval graph and label lane (DESIGN.md "Eval graph"), a lab placeholder at real size: the
 * win-chance line over the whole game, a pen ring on every marked move (thin for inaccuracies),
 * the cursor on the move shown, and the key move's label with its leader tick.
 */
function EvalGraph({ reduced }: { reduced: boolean }) {
  const [plotRef, size] = useMeasure<HTMLDivElement>();
  const cursorRef = useRef<HTMLDivElement>(null);
  const W = size.width;
  const H = size.height;
  const padX = 10;
  const padY = 9;
  const n = plies.length;
  const x = (ply: number) => padX + (ply / n) * Math.max(0, W - 2 * padX);
  const y = (wc: number) => padY + ((1 - wc) / 2) * Math.max(0, H - 2 * padY);
  const pts: Pt[] = [[x(0), y(0)], ...plies.map((p): Pt => [x(p.ply), y(winChance(p))])];
  const zero = y(0);
  const area = `${line(pts)} L${f2(x(n))} ${f2(zero)} L${f2(x(0))} ${f2(zero)}Z`;
  const kx = x(keyPly.ply);
  const ky = y(wcAfter);
  useAt(reduced, T.contact, () => {
    const el = cursorRef.current;
    if (!el) return undefined;
    const run = animate(el, { x: [0, kx - x(prevPly.ply)] }, spring.swing);
    return () => run.stop();
  });
  const label = `${prefix} ${keyPly.san}${keyGlyph}`;
  return (
    <figure className="w-full">
      <figcaption className="sr-only">
        White's win chance through the game. {label} took it from {Math.round(shareBefore * 100)}%
        to {Math.round(shareAfter * 100)}%.
      </figcaption>
      {/* Label lane: 22px, never on the line. */}
      <div className="relative h-5.5" aria-hidden>
        {W > 0 && (
          <motion.span
            className="absolute bottom-0 -translate-x-1/2 font-serif text-sm font-semibold whitespace-nowrap text-pen-text tabular-nums lining-nums"
            style={{ left: Math.min(Math.max(kx, 44), W - 44) }}
            initial={reduced ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={
              reduced
                ? { duration: 0 }
                : { delay: sec(T.ring) + dur.drawSmall / 2, duration: dur.drawSmall / 2 }
            }
          >
            {prefix}&nbsp;{keyPly.san}
            <span className="tracking-glyph">{keyGlyph}</span>
          </motion.span>
        )}
      </div>
      <div ref={plotRef} className="relative h-22 rounded-board bg-well md:h-30">
        {W > 0 && (
          <svg className="absolute inset-0 size-full overflow-visible" aria-hidden>
            <path
              d={`M${f2(x(0))} ${f2(zero)} L${f2(x(n))} ${f2(zero)}`}
              className="stroke-rule-strong"
              strokeWidth={1}
            />
            <path d={area} className="fill-ink/10" />
            <path
              d={line(pts)}
              fill="none"
              className="stroke-ink"
              strokeWidth={2}
              strokeLinejoin="round"
            />
            {/* Leader tick from the label lane down to the key ring. */}
            <motion.path
              d={`M${f2(kx)} -2 L${f2(kx)} ${f2(ky - 10)}`}
              className="stroke-rule-strong"
              strokeWidth={1}
              initial={reduced ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={
                reduced
                  ? { duration: 0 }
                  : { delay: sec(T.ring) + dur.drawSmall / 2, duration: dur.drawSmall / 2 }
              }
            />
            {plies.map((p) => {
              if (!p.judgement) return null;
              const cx = x(p.ply);
              const cy = y(winChance(p));
              const ring = ringPoints(cx, cy, 7 * 1.12, 7, 10, 1);
              // Pen on `well` holds 4.14:1 on its own, so the small graph rings drop the halo.
              const scale = p.judgement === "inaccuracy" ? 0.75 : 1;
              const isKey = p.ply === keyPly.ply;
              return (
                <PenStroke
                  key={p.ply}
                  reduced={reduced || !isKey}
                  segments={[ring.main, ring.tail]}
                  shape={outline(ring.all, scale)}
                  maskWidth={3.2 * scale + 5}
                  at={T.ring}
                  duration={dur.drawSmall}
                  halo={false}
                />
              );
            })}
          </svg>
        )}
        {/* Cursor: a 1.5px ink rule on the move shown; it steps to the key move at contact. */}
        {W > 0 && (
          <div
            ref={cursorRef}
            aria-hidden
            className="absolute inset-y-0 w-[1.5px] bg-ink"
            style={{
              left: x(prevPly.ply) - 0.75,
              transform: reduced ? `translateX(${kx - x(prevPly.ply)}px)` : undefined,
            }}
          />
        )}
      </div>
    </figure>
  );
}

/**
 * The scoresheet ring (the board stroke at 0.8×, fitted to the word: rx = w/2 + 10, ry 14) and,
 * when both ends are in view at T.link, the link mark: one pen stroke from that ring up into the
 * gutter, along it, and into the board over the empty square below the target (DESIGN.md "Link").
 */
function SheetMarks({
  reduced,
  link,
  word,
  board,
  well,
  sheet,
}: {
  reduced: boolean;
  link: boolean;
  word: Box;
  board: Box;
  well: Box;
  sheet: Box;
}) {
  const cx = word.x + word.w / 2;
  const cy = word.y + word.h / 2;
  const rx = word.w / 2 + 10;
  const ry = 14;
  const ring = ringPoints(cx, cy, rx, ry, -3, 2.5);
  const sq = board.w / 8;
  const scale = (0.9 * sq) / 56; // 0.9× the board pen at this square size
  const tx = board.x + (to.charCodeAt(0) - 97 + 0.5) * sq;
  const ty = board.y + (8 - Number(to[1]) + 0.5) * sq;
  const dir = tx >= cx ? 1 : -1;
  const sx = cx + dir * 0.35 * rx;
  const sy = cy - 0.94 * ry - 6;
  const gy = (well.y + well.h + sheet.y) / 2; // the middle of the gutter
  const end: Pt = [tx, ty + 0.38 * sq + 6];
  const r = Math.min(10, Math.abs(tx - sx) / 2);
  const k = 0.5523; // quarter-circle control distance
  const path: Pt[] = [
    ...cubic([sx, sy], [sx, sy], [sx, gy + r], [sx, gy + r], 6),
    ...cubic(
      [sx, gy + r],
      [sx, gy + r - k * r],
      [sx + dir * (r - k * r), gy],
      [sx + dir * r, gy],
      8,
    ).slice(1),
    ...cubic(
      [sx + dir * r, gy],
      [sx + dir * r, gy],
      [tx - dir * r, gy],
      [tx - dir * r, gy],
      12,
    ).slice(1),
    ...cubic(
      [tx - dir * r, gy],
      [tx - dir * (r - k * r), gy],
      [tx, gy - r + k * r],
      [tx, gy - r],
      8,
    ).slice(1),
    ...cubic([tx, gy - r], [tx, gy - r], end, end, 12).slice(1),
  ];
  return (
    <svg className="pointer-events-none absolute inset-0 size-full overflow-visible" aria-hidden>
      <PenStroke
        reduced={reduced}
        segments={[ring.main, ring.tail]}
        shape={outline(ring.all, 0.8)}
        maskWidth={3.2 * 0.8 + 5}
        at={T.ring}
        duration={dur.drawSmall}
      />
      {link && (
        <PenStroke
          reduced={reduced}
          segments={[path]}
          shape={outline(path, scale)}
          maskWidth={3.2 * scale + 5}
          at={0}
          duration={dur.draw}
        />
      )}
    </svg>
  );
}

function KeyMove({ reduced }: { reduced: boolean }) {
  return (
    <>
      <span data-link-word className="inline-block">
        {keyPly.san}
        <motion.span className="tracking-glyph text-pen-text" {...appear(reduced, T.ring)}>
          {keyGlyph}
        </motion.span>
      </span>
      <motion.span className="ml-5 font-normal text-ink-faint" {...appear(reduced, T.ring)}>
        {keyEval}
      </motion.span>
    </>
  );
}

/** The link is drawn only when both of its ends are fully in view at T.link (DESIGN.md "Link"). */
function useLinkInView(rootRef: { current: HTMLDivElement | null }, reduced: boolean) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const ends = [
      root.querySelector("[data-link-word]"),
      root.querySelector("[data-link-target]"),
    ].filter((e): e is Element => e !== null);
    const seen = new Map<Element, boolean>();
    let due = false;
    let decided = false;
    const decide = () => {
      if (decided || !due || seen.size < ends.length) return;
      decided = true;
      setOn(ends.every((e) => seen.get(e)));
    };
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) seen.set(e.target, e.intersectionRatio >= 0.99);
        decide();
      },
      { threshold: [0, 0.99, 1] },
    );
    for (const e of ends) io.observe(e);
    const timer = setTimeout(
      () => {
        due = true;
        decide();
      },
      reduced ? 0 : T.link,
    );
    return () => {
      io.disconnect();
      clearTimeout(timer);
    };
  }, [rootRef, reduced]);
  return on;
}

const targetBox: CSSProperties = {
  left: `${(to.charCodeAt(0) - 97) * 12.5}%`,
  top: `${(8 - Number(to[1])) * 12.5}%`,
  width: "12.5%",
  height: "12.5%",
};

function Hero({ reduced }: { reduced: boolean }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [boxes, setBoxes] = useState<{ word: Box; board: Box; well: Box; sheet: Box } | null>(null);
  const link = useLinkInView(rootRef, reduced);
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const measure = () => {
      const get = (sel: string) => boxOf(root.querySelector(sel), root);
      const word = get("[data-link-word]");
      const board = get("[data-link-board]");
      const well = get("[data-link-well]");
      const sheet = get("[data-link-sheet]");
      setBoxes(word && board && well && sheet ? { word, board, well, sheet } : null);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    void document.fonts?.ready.then(measure);
    return () => ro.disconnect();
  }, []);

  // From lg the left column is the bar, an 8px gap and the board in its 8px well.
  const cols = "lg:grid-cols-[calc(var(--board-size)+2.5rem)_minmax(0,1fr)] lg:gap-x-8 xl:gap-x-16";
  return (
    <div className="mx-auto max-w-7xl px-(--space-gutter) pt-6 pb-16 md:pt-10">
      <div ref={rootRef} className={`relative grid grid-cols-1 gap-y-6 ${cols}`}>
        <header className="max-w-[60ch] lg:col-start-2 lg:row-start-1 lg:self-start">
          <h1 className="font-serif text-3xl font-semibold">{title}</h1>
          <p className="mt-2 text-ink-soft">{context}</p>
          <p className="mt-1 text-ink-soft">
            The board shows the position after {prefix}&nbsp;{keyPly.san}, {toMove} to move.
          </p>
        </header>

        <div className="lg:col-start-1 lg:row-span-2 lg:row-start-1">
          <div className="mb-2 w-(--board-size) md:hidden">
            <EvalBar reduced={reduced} vertical={false} />
          </div>
          <div className="flex items-start gap-2">
            <div className="hidden md:mt-2 md:block">
              <EvalBar reduced={reduced} vertical />
            </div>
            <div data-link-well className="md:rounded-board md:bg-well md:p-2">
              <div data-link-board className="relative w-(--board-size)">
                <Board reduced={reduced} className="w-(--board-size) rounded-board" />
                <span
                  data-link-target
                  aria-hidden
                  className="pointer-events-none absolute"
                  style={targetBox}
                />
              </div>
            </div>
          </div>
        </div>

        {/* The scoresheet: reference 3's grid, with its comment in the sheet's right margin. */}
        <div
          data-link-sheet
          className="rounded-sheet bg-sheet p-(--space-panel-pad) lg:col-span-2 lg:row-start-3 lg:grid lg:grid-cols-[calc(var(--board-size)+2.5rem-var(--space-panel-pad))_minmax(0,1fr)] lg:items-start lg:gap-x-8 xl:gap-x-16"
        >
          <table className="w-full table-fixed border-collapse font-serif tabular-nums lining-nums">
            <caption className="sr-only">
              Moves {sheetRows[0]?.n} to {sheetRows.at(-1)?.n}
            </caption>
            <colgroup>
              <col className="w-10 sm:w-14" />
              <col />
              <col />
            </colgroup>
            <thead className="sr-only">
              <tr>
                <th scope="col">Move</th>
                <th scope="col">White</th>
                <th scope="col">Black</th>
              </tr>
            </thead>
            <tbody>
              {sheetRows.map((row) => {
                const isKey = row.n === keyNo;
                const cell = (ply: Row["white"], side: "white" | "black") =>
                  isKey && keyPly.color === side ? (
                    <KeyMove reduced={reduced} />
                  ) : (
                    <San ply={ply} />
                  );
                return (
                  <tr
                    key={row.n}
                    className="h-(--space-scoresheet-row) border-b border-rule last:border-b-0"
                  >
                    <td className="relative border-r border-rule pr-3 text-right text-ink-faint">
                      {isKey && (
                        <motion.span
                          aria-hidden
                          className="absolute inset-y-0.5 right-0 left-0 rounded-l-field bg-well"
                          {...appear(reduced, T.select)}
                        />
                      )}
                      <span className="relative">{row.n}</span>
                    </td>
                    <td className="relative border-r border-rule pr-2 pl-5 font-semibold">
                      {isKey && (
                        <motion.span
                          aria-hidden
                          className="absolute inset-x-0 inset-y-0.5 bg-well"
                          {...appear(reduced, T.select)}
                        />
                      )}
                      <span className="relative whitespace-nowrap">{cell(row.white, "white")}</span>
                    </td>
                    <td className="relative pr-2 pl-5 font-semibold">
                      {isKey && (
                        <motion.span
                          aria-hidden
                          className="absolute inset-y-0.5 right-0 left-0 rounded-r-field bg-well"
                          {...appear(reduced, T.select)}
                        />
                      )}
                      <span className="relative whitespace-nowrap">{cell(row.black, "black")}</span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>

          {/* The margin comment, level with the ringed row (under the rows on narrow screens). */}
          <motion.p
            className="mt-4 max-w-[52ch] font-serif-italic text-prose text-pretty text-ink italic lg:mt-0 lg:pt-0.5"
            {...appear(reduced, T.note)}
          >
            {note}
          </motion.p>
        </div>

        <div className="lg:col-start-2 lg:row-start-2 lg:self-end">
          <EvalGraph reduced={reduced} />
        </div>

        {boxes && <SheetMarks reduced={reduced} link={link} {...boxes} />}
      </div>
    </div>
  );
}

export default function StyleguideHeroA() {
  const beat = useBeat();
  return (
    <LabFrame
      surface="styleguide-hero"
      variant="a"
      title="Scoresheet margin"
      rationale="The board heads the scoresheet; the red pen rings the move on its row and on its square, one link stroke ties the two, the eval bar and graph swing with the move, and the coach's comment sits in the sheet's margin."
    >
      <div className="mx-auto flex max-w-7xl justify-end px-(--space-gutter)">
        <Replay onClick={beat.replay} />
      </div>
      <Hero key={beat.key} reduced={beat.reduced} />
    </LabFrame>
  );
}
