"use client";

import { Curve, Responsive, Shape } from "@visx/visx";
import {
  type KeyboardEvent,
  type PointerEvent,
  useCallback,
  useId,
  useMemo,
  useState,
} from "react";
import { cn } from "../lib/index.js";
import { dur, ease, motion, spring, useReducedMotion } from "../motion/index.js";
import { plyForX, xForPly, yForWin } from "./scales.js";

export type EvalGraphJudgement = "inaccuracy" | "mistake" | "blunder";

export interface EvalGraphPoint {
  ply: number;
  /** White's win percent after the move, 0 to 100. */
  winWhite: number;
  san: string;
  judgement?: EvalGraphJudgement | null;
}

export interface EvalGraphProps {
  points: EvalGraphPoint[];
  /** Accessible name of the graph, for example "Win chance, DrNykterstein v Night-King96". */
  label: string;
  currentPly?: number;
  onSelectPly?: (ply: number) => void;
  phases?: { middlegameStartPly?: number; endgameStartPly?: number };
  playerColor?: "white" | "black";
  /** Plot height in px. Defaults to DESIGN.md's 120px, or 88px below a 480px-wide container. */
  height?: number;
  /** Reveal left to right on mount over 2 × `dur.reveal` (no reveal under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

// Layout, from DESIGN.md "Eval graph": a 22px label lane above the plot; a phase lane below it.
const LANE = 22;
const AXIS = 18;
const PAD = 8;
const LABEL_GAP = 10;
const MAX_PUSH = 56;
const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];
const GLYPH = { blunder: "??", mistake: "?", inaccuracy: "?!" } as const;

const isSerious = (j: EvalGraphPoint["judgement"]) => j === "mistake" || j === "blunder";
const moveNumber = (ply: number) => Math.ceil(ply / 2);
const sideOf = (ply: number) => (ply % 2 === 1 ? "White" : "Black");

/** Notation with the move number: `16. Rxe6??`, `9…b5?`. */
function notation(p: EvalGraphPoint): { prefix: string; san: string; glyph: string } {
  const prefix = p.ply % 2 === 1 ? `${moveNumber(p.ply)}. ` : `${moveNumber(p.ply)}…`;
  return { prefix, san: p.san, glyph: p.judgement ? GLYPH[p.judgement] : "" };
}

/** The slider's value text: "Move 16, White Rxe6, blunder, White 22%". */
export function plyText(p: EvalGraphPoint): string {
  const parts = [`Move ${moveNumber(p.ply)}`, `${sideOf(p.ply)} ${p.san}`];
  if (p.judgement) parts.push(p.judgement);
  parts.push(`White ${Math.round(p.winWhite)}%`);
  return parts.join(", ");
}

// Literata 600 at 14px averages about 0.6em a character; labels only need a safe estimate.
const textWidth = (s: string) => Math.ceil(s.length * 8.4);

interface PlacedLabel {
  key: number;
  x: number;
  tickX: number;
  tickTop: number;
  serious: boolean;
  parts: { prefix: string; san: string; glyph: string };
}

/**
 * Places move labels in the lane, left to right, pushed right to keep a 10px gap. A label that
 * would drift too far from its tick falls back to a shorter form (no move number, then the glyph
 * only), and is dropped if even that won't fit; the text alternative still lists every move.
 * Inaccuracies are labelled only on wide graphs, and only where they fit without pushing.
 */
function placeLabels(
  marks: { p: EvalGraphPoint; x: number; y: number }[],
  width: number,
  wide: boolean,
): PlacedLabel[] {
  const placed: PlacedLabel[] = [];
  const taken: [number, number][] = [];
  const free = (a: number, b: number) =>
    taken.every(([s, e]) => b + LABEL_GAP <= s || a >= e + LABEL_GAP);
  let lastEnd = -Infinity;

  for (const m of marks) {
    if (!isSerious(m.p.judgement)) continue;
    const full = notation(m.p);
    const forms = [
      full,
      { prefix: "", san: full.san, glyph: full.glyph },
      { prefix: "", san: "", glyph: full.glyph },
    ];
    for (const parts of forms) {
      const w = textWidth(parts.prefix + parts.san + parts.glyph);
      let start = Math.max(m.x - 6, lastEnd + LABEL_GAP, 0);
      if (start + w > width) start = width - w;
      if (start < lastEnd + LABEL_GAP || start - m.x > MAX_PUSH || m.x - start > w + 6) continue;
      placed.push({ key: m.p.ply, x: start, tickX: m.x, tickTop: m.y, serious: true, parts });
      taken.push([start, start + w]);
      lastEnd = start + w;
      break;
    }
  }
  if (wide) {
    for (const m of marks) {
      if (m.p.judgement !== "inaccuracy") continue;
      const parts = notation(m.p);
      const w = textWidth(parts.prefix + parts.san + parts.glyph);
      const start = Math.min(Math.max(m.x - 6, 0), width - w);
      if (!free(start, start + w)) continue;
      placed.push({ key: m.p.ply, x: start, tickX: m.x, tickTop: m.y, serious: false, parts });
      taken.push([start, start + w]);
    }
  }
  return placed;
}

function summary(points: EvalGraphPoint[], playerColor?: "white" | "black"): string {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return "";
  const marked = points.filter((p) => p.judgement);
  const you = playerColor ? ` You played ${playerColor === "white" ? "White" : "Black"}.` : "";
  const list = marked.length
    ? ` Marked moves: ${marked.map((p) => plyText(p)).join("; ")}.`
    : " No moves are marked.";
  return `${points.length} half-moves. White's win chance goes from ${Math.round(first.winWhite)}% to ${Math.round(last.winWhite)}%.${you}${list}`;
}

export function EvalGraph({
  points,
  label,
  currentPly,
  onSelectPly,
  phases,
  playerColor,
  height,
  drawIn = false,
  className,
}: EvalGraphProps) {
  const reduced = useReducedMotion();
  const rawId = useId();
  const id = `eg${rawId.replace(/[^a-zA-Z0-9]/g, "")}`;
  const { parentRef, width } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });
  const [ownPly, setOwnPly] = useState<number | undefined>(undefined);
  const [hoverPly, setHoverPly] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);

  const count = points.length;
  const last = points[count - 1]?.ply ?? 1;
  const active = currentPly ?? ownPly;
  const plotH = height ?? (width > 0 && width < 480 ? 88 : 120);
  const totalH = LANE + plotH + AXIS;
  const innerW = Math.max(0, width - 2 * PAD);
  const x = useCallback((ply: number) => PAD + xForPly(ply, innerW, count), [innerW, count]);
  const y = useCallback((win: number) => LANE + yForWin(win, plotH), [plotH]);

  const select = useCallback(
    (ply: number) => {
      const next = Math.min(last, Math.max(1, Math.round(ply)));
      if (currentPly === undefined) setOwnPly(next);
      onSelectPly?.(next);
    },
    [currentPly, last, onSelectPly],
  );

  const marks = useMemo(
    () => points.filter((p) => p.judgement).map((p) => ({ p, x: x(p.ply), y: y(p.winWhite) })),
    [points, x, y],
  );
  const labels = useMemo(
    () => (width > 0 ? placeLabels(marks, width, width >= 768) : []),
    [marks, width],
  );
  const describe = useMemo(() => summary(points, playerColor), [points, playerColor]);

  if (count === 0) {
    return (
      <figure className={cn("m-0", className)} aria-label={label}>
        <div
          className="flex items-center justify-center rounded-board bg-well text-sm text-ink-soft"
          style={{ height: LANE + 88 + AXIS }}
        >
          No moves to graph yet
        </div>
      </figure>
    );
  }

  const pointAt = (ply: number) => points.find((p) => p.ply === ply);
  const activePoint = active === undefined ? undefined : pointAt(active);
  const shown = hoverPly ?? (focused ? (active ?? last) : null);
  const shownPoint = shown === null ? undefined : pointAt(shown);
  const valuePoint = activePoint ?? points[count - 1];

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const from = active ?? last;
    const steps: Record<string, number> = {
      ArrowRight: from + 1,
      ArrowUp: from + 1,
      ArrowLeft: from - 1,
      ArrowDown: from - 1,
      PageUp: from + 10,
      PageDown: from - 10,
      Home: 1,
      End: last,
    };
    const next = steps[e.key];
    if (next === undefined) return;
    e.preventDefault();
    select(next);
  };

  const plyFromEvent = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return points[plyForX(e.clientX - rect.left - PAD, innerW, count) - 1]?.ply ?? 1;
  };

  const baseline = y(50);
  const phaseLines = [
    { ply: phases?.middlegameStartPly, name: "Middlegame" },
    { ply: phases?.endgameStartPly, name: "Endgame" },
  ].flatMap((ph) =>
    ph.ply && ph.ply > 1 && ph.ply <= last
      ? [{ name: ph.name, x: (x(ph.ply - 1) + x(ph.ply)) / 2 }]
      : [],
  );
  let phaseEnd = -Infinity;
  const phaseLabels = phaseLines.map((ph) => {
    const w = textWidth(ph.name) * 0.8;
    let lx = Math.max(ph.x + 4, phaseEnd + 8);
    if (lx + w > width) lx = Math.max(phaseEnd + 8, width - w);
    phaseEnd = lx + w;
    return { ...ph, lx };
  });

  const reveal = drawIn && !reduced;
  const clip = reveal ? `url(#${id}-clip)` : undefined;
  const sideLabel = (side: "white" | "black") =>
    `${side === "white" ? "White" : "Black"}${playerColor === side ? " (you)" : ""}`;

  return (
    <figure className={cn("m-0", className)}>
      <div ref={parentRef} className="relative w-full" style={{ height: totalH }}>
        {width > 0 && (
          <svg width={width} height={totalH} aria-hidden="true" className="block overflow-visible">
            {reveal && (
              <defs>
                <clipPath id={`${id}-clip`}>
                  <motion.rect
                    x={0}
                    y={0}
                    width={width}
                    height={totalH}
                    style={{ originX: 0, originY: 0 }}
                    initial={{ scaleX: 0 }}
                    animate={{ scaleX: 1 }}
                    transition={{ duration: 2 * dur.reveal, ease: ease.pen }}
                  />
                </clipPath>
              </defs>
            )}
            <rect x={0} y={LANE} width={width} height={plotH} rx={2} className="fill-well" />
            <text x={PAD} y={LANE + 14} className="fill-ink-faint font-sans text-xs">
              {sideLabel("white")}
            </text>
            <text x={PAD} y={LANE + plotH - 6} className="fill-ink-faint font-sans text-xs">
              {sideLabel("black")}
            </text>
            <line
              x1={0}
              x2={width}
              y1={baseline}
              y2={baseline}
              strokeWidth={1}
              className="stroke-rule-strong"
            />
            {phaseLines.map((ph) => (
              <line
                key={ph.name}
                data-phase={ph.name}
                x1={ph.x}
                x2={ph.x}
                y1={LANE}
                y2={LANE + plotH}
                strokeWidth={1}
                className="stroke-rule-strong"
              />
            ))}
            {phaseLabels.map((ph) => (
              <text
                key={ph.name}
                x={ph.lx}
                y={LANE + plotH + 14}
                className="fill-ink-faint font-sans text-xs"
              >
                {ph.name}
              </text>
            ))}
            <g clipPath={clip}>
              <Shape.Area<EvalGraphPoint>
                data={points}
                x={(p) => x(p.ply)}
                y0={baseline}
                y1={(p) => y(p.winWhite)}
                curve={Curve.curveMonotoneX}
                className="fill-ink/10"
              />
              <Shape.LinePath<EvalGraphPoint>
                data={points}
                x={(p) => x(p.ply)}
                y={(p) => y(p.winWhite)}
                curve={Curve.curveMonotoneX}
                data-series="line"
                fill="none"
                strokeWidth={2}
                strokeLinejoin="round"
                strokeLinecap="round"
                className="stroke-ink"
              />
              {labels.map((l) => {
                const anchor = Math.min(
                  Math.max(l.tickX, l.x + 2),
                  l.x + textWidth(l.parts.prefix + l.parts.san + l.parts.glyph) - 2,
                );
                return (
                  <g key={l.key}>
                    <line
                      x1={anchor}
                      y1={LANE - 3}
                      x2={l.tickX}
                      y2={l.tickTop - (l.serious ? 9 : 7)}
                      strokeWidth={1}
                      className="stroke-rule-strong"
                    />
                    <text
                      x={l.x}
                      y={15}
                      className={cn(
                        "font-serif text-sm font-semibold stroke-sheet",
                        l.serious ? "fill-ink" : "fill-ink-soft",
                      )}
                      style={{
                        fontVariantNumeric: "lining-nums tabular-nums",
                        paintOrder: "stroke",
                        strokeWidth: 4,
                        strokeLinejoin: "round",
                      }}
                    >
                      {l.parts.prefix}
                      {l.parts.san}
                      <tspan
                        className={cn(
                          "tracking-glyph",
                          l.serious ? "fill-pen-text" : "fill-ink-soft",
                        )}
                      >
                        {l.parts.glyph}
                      </tspan>
                    </text>
                  </g>
                );
              })}
              {marks.map(({ p, x: mx, y: my }) => {
                const serious = isSerious(p.judgement);
                const half = p.judgement === "blunder" ? 9 : serious ? 7 : 5;
                const w = p.judgement === "blunder" ? 3 : serious ? 2.5 : 1.5;
                return (
                  <g key={p.ply} data-mark={serious ? "pen" : "pencil"} data-ply={p.ply}>
                    <line
                      x1={mx}
                      x2={mx}
                      y1={my - half}
                      y2={my + half}
                      strokeWidth={w + 4}
                      strokeLinecap="round"
                      className="stroke-well"
                    />
                    <line
                      x1={mx}
                      x2={mx}
                      y1={my - half}
                      y2={my + half}
                      strokeWidth={w}
                      strokeLinecap="round"
                      className={serious ? "stroke-pen" : "stroke-pencil"}
                    />
                  </g>
                );
              })}
            </g>
            {shownPoint && shown !== active && (
              <line
                x1={x(shownPoint.ply)}
                x2={x(shownPoint.ply)}
                y1={LANE}
                y2={LANE + plotH}
                strokeWidth={1}
                className="stroke-pencil"
              />
            )}
            {activePoint && (
              <motion.g
                initial={false}
                animate={{ x: x(activePoint.ply), y: y(activePoint.winWhite) }}
                transition={reduced ? { duration: 0 } : spring.swing}
                data-cursor={activePoint.ply}
              >
                <line
                  x1={0}
                  x2={0}
                  y1={LANE - y(activePoint.winWhite)}
                  y2={LANE + plotH - y(activePoint.winWhite)}
                  strokeWidth={1.5}
                  className="stroke-ink"
                />
                <circle r={4} strokeWidth={2} className="fill-ink stroke-well" />
              </motion.g>
            )}
          </svg>
        )}
        <div
          role="slider"
          tabIndex={0}
          aria-label={label}
          aria-valuemin={1}
          aria-valuemax={last}
          aria-valuenow={valuePoint?.ply ?? last}
          aria-valuetext={valuePoint ? plyText(valuePoint) : undefined}
          aria-describedby={`${id}-desc`}
          className="absolute inset-x-0 cursor-crosshair touch-pan-y rounded-board focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          style={{ top: LANE, height: plotH }}
          onKeyDown={onKeyDown}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture?.(e.pointerId);
            select(plyFromEvent(e));
          }}
          onPointerMove={(e) => {
            const ply = plyFromEvent(e);
            setHoverPly(ply);
            if (e.buttons === 1 && ply !== active) select(ply);
          }}
          onPointerLeave={() => setHoverPly(null)}
        />
        {shownPoint && width > 0 && (
          <Readout point={shownPoint} x={x(shownPoint.ply)} width={width} top={LANE + 6} />
        )}
      </div>
      <p id={`${id}-desc`} className="sr-only">
        {describe}
      </p>
    </figure>
  );
}

const READOUT_W = 148;

function Readout({
  point,
  x,
  width,
  top,
}: {
  point: EvalGraphPoint;
  x: number;
  width: number;
  top: number;
}) {
  const parts = notation(point);
  const serious = isSerious(point.judgement);
  const flip = x + 12 + READOUT_W > width;
  const left = flip ? Math.max(0, x - 12 - READOUT_W) : x + 12;
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute flex flex-col rounded-field bg-sheet px-3 py-1.5 shadow-lift"
      style={{ left, top, width: READOUT_W }}
    >
      <span
        className="font-serif text-sm font-semibold text-ink"
        style={{ fontVariantNumeric: "lining-nums tabular-nums" }}
      >
        {parts.prefix}
        {parts.san}
        <span className={cn("tracking-glyph", serious ? "text-pen-text" : "text-ink-soft")}>
          {parts.glyph}
        </span>
      </span>
      <span className="font-sans text-xs text-ink-soft">
        White {Math.round(point.winWhite)}%{point.judgement ? `, ${point.judgement}` : ""}
      </span>
    </div>
  );
}
