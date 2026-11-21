"use client";

import { Curve, Responsive, Shape } from "@visx/visx";
import {
  type CSSProperties,
  type KeyboardEvent,
  memo,
  type PointerEvent,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "../lib/index.js";
import {
  animate,
  dur,
  ease,
  type MotionValue,
  motion,
  spring,
  useMotionValue,
  useReducedMotion,
} from "../motion/index.js";
import {
  AXIS,
  cornerWidth,
  GLYPH,
  GRAPH_NARROW,
  LABEL_WIDE,
  LANE,
  type LabelMark,
  leaderPoints,
  movePrefix,
  PAD,
  PLOT_HEIGHT,
  type PlacedLabel,
  placeCorners,
  placeLabels,
  plyForX,
  READOUT_H,
  READOUT_W,
  readoutPlacement,
  type Side,
  xForPly,
  yForWin,
} from "./scales.js";

export type EvalGraphJudgement = "inaccuracy" | "mistake" | "blunder";

export interface EvalGraphPoint {
  ply: number;
  /** White's win percent after the move, 0 to 100. */
  winWhite: number;
  san: string;
  judgement?: EvalGraphJudgement | null;
}

export interface EvalGraphProps {
  /** One point per ply, in game order. */
  points: EvalGraphPoint[];
  /** Accessible name of the graph, for example "Win chance, DrNykterstein v Night-King96". */
  label: string;
  /**
   * The ply shown on the board. 0 is the start position: no cursor, and the slider reads "Start
   * position" (it still reports 1, its minimum; ArrowRight then selects ply 1).
   */
  currentPly?: number;
  onSelectPly?: (ply: number) => void;
  phases?: { middlegameStartPly?: number; endgameStartPly?: number };
  playerColor?: "white" | "black";
  /**
   * Plot height in px. By default the plot is 120px (DESIGN.md), or 88px in a container narrower
   * than `--container-graph` (496px), set by a container query so nothing shifts on measuring.
   */
  height?: number;
  /** Reveal left to right on mount over 2 × `dur.reveal` (no reveal under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

/** Marks fade in over this many px as the reveal passes them. */
const FADE_SPAN = 24;
const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];
/** Text halo for labels: the surface the graph sits on (set `--eval-graph-surface` off sheet). */
const HALO = "var(--eval-graph-surface, var(--color-sheet))";
const NUMERALS = { fontVariantNumeric: "lining-nums tabular-nums" } as const;
const CORNER_HALO = { strokeWidth: 3, strokeLinejoin: "round", paintOrder: "stroke" } as const;
/** A tap may wander this far in total before it stops counting as a tap. */
const TAP_SLOP = 10;
/** Horizontal travel that turns a touch into a scrub. */
const SCRUB_START = 6;

const isSerious = (j: EvalGraphPoint["judgement"]) => j === "mistake" || j === "blunder";
const sideOf = (ply: number) => (ply % 2 === 1 ? "White" : "Black");

/**
 * Opacity on the reveal clock: `--wipe` runs 0 to 1 with the clip, and an element at `x` fades in
 * over FADE_SPAN px as the wipe passes it. With no reveal running `--wipe` is 1, so everything,
 * including marks that arrive later, is fully drawn at once.
 */
const wipeOpacity = (x: number, width: number): CSSProperties => ({
  opacity:
    `clamp(0, calc((var(--wipe, 1) * ${width + FADE_SPAN} - ${x.toFixed(1)}) / ${FADE_SPAN}), 1)` as unknown as number,
});

/** The slider's value text: "Move 16, White Rxe6, blunder, White 22%". */
export function plyText(p: EvalGraphPoint): string {
  const parts = [`Move ${Math.ceil(p.ply / 2)}`, `${sideOf(p.ply)} ${p.san}`];
  if (p.judgement) parts.push(p.judgement);
  parts.push(`White ${Math.round(p.winWhite)}%`);
  return parts.join(", ");
}

function summary(points: EvalGraphPoint[], playerColor?: "white" | "black"): string {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return "";
  const serious = points.filter((p) => isSerious(p.judgement));
  const inaccuracies = points.filter((p) => p.judgement === "inaccuracy").length;
  const you = playerColor ? ` You played ${playerColor === "white" ? "White" : "Black"}.` : "";
  const list = serious.length
    ? ` Mistakes and blunders: ${serious.map(plyText).join("; ")}.`
    : " No mistakes or blunders.";
  const light = inaccuracies
    ? ` Also ${inaccuracies} inaccurac${inaccuracies === 1 ? "y" : "ies"}.`
    : "";
  return `${points.length} half-moves. White's win chance goes from ${Math.round(first.winWhite)}% to ${Math.round(last.winWhite)}%.${you}${list}${light}`;
}

interface Mark {
  ply: number;
  x: number;
  y1: number;
  y2: number;
  judgement: EvalGraphJudgement;
  /** Whose move: the player's, the opponent's (false: a pencil hairline) or unknown (null). */
  mine: boolean | null;
}

interface Phase {
  name: string;
  x: number;
  labelX: number;
}

interface Corner {
  text: string;
  side: Side;
  /** The text's rendered width in px. */
  width: number;
}

/** The parts of the graph that only change with the data or the size. */
const StaticLayer = memo(function StaticLayer({
  width,
  plotH,
  linePath,
  areaPath,
  baseline,
  marks,
  labels,
  phases,
  corners,
  progress,
  reveal,
  clipId,
}: {
  width: number;
  plotH: number;
  linePath: string;
  areaPath: string;
  baseline: number;
  marks: Mark[];
  labels: PlacedLabel[];
  phases: Phase[];
  corners: { top: Corner; bottom: Corner };
  progress: MotionValue<number>;
  /** Clip the line and area to the reveal's wipe; without a reveal there is no clip at all. */
  reveal: boolean;
  clipId: string;
}) {
  const clip = reveal ? `url(#${clipId})` : undefined;
  const cornerText = (c: Corner, y: number, key: string) => (
    <text
      key={key}
      data-corner={key}
      data-side={c.side}
      x={c.side === "left" ? PAD : width - PAD}
      y={y}
      textAnchor={c.side === "left" ? "start" : "end"}
    >
      {c.text}
    </text>
  );

  return (
    <svg
      width={width}
      height={LANE + plotH + AXIS}
      aria-hidden="true"
      className="absolute inset-0 block"
    >
      {reveal && (
        <defs>
          <clipPath id={clipId}>
            <motion.rect
              x={0}
              y={0}
              width={width}
              height={LANE + plotH + AXIS}
              style={{ scaleX: progress, originX: 0, originY: 0 }}
            />
          </clipPath>
        </defs>
      )}
      <rect x={0} y={LANE} width={width} height={plotH} rx={2} className="fill-well" />
      <g clipPath={clip}>
        <path d={areaPath} className="fill-ink/10" />
      </g>
      <line
        x1={0}
        x2={width}
        y1={baseline}
        y2={baseline}
        strokeWidth={1}
        className="stroke-rule-strong"
      />
      {phases.map((ph) => (
        <line
          key={ph.name}
          data-phase={ph.name}
          x1={ph.x}
          x2={ph.x}
          y1={LANE}
          y2={LANE + plotH}
          strokeWidth={1}
          className="stroke-pencil"
        />
      ))}
      <g className="group-data-readout:invisible">
        {phases.map((ph) => (
          <text
            key={ph.name}
            x={ph.labelX}
            y={LANE + plotH + 15}
            className="fill-ink-faint font-sans text-xs"
          >
            {ph.name}
          </text>
        ))}
      </g>
      <g clipPath={clip}>
        <path
          d={linePath}
          data-series="line"
          fill="none"
          strokeWidth={2}
          strokeLinejoin="round"
          strokeLinecap="round"
          className="stroke-ink"
        />
      </g>
      <g data-layer="leaders">
        {labels
          .filter((l) => l.leader)
          .map((l) => (
            <polyline
              key={l.ply}
              data-leader={l.ply}
              points={leaderPoints(l)
                .map(([x, y]) => `${x},${y}`)
                .join(" ")}
              fill="none"
              strokeWidth={1}
              strokeLinejoin="round"
              className="stroke-pencil"
              style={wipeOpacity(l.tickX, width)}
            />
          ))}
      </g>
      {/* Corner labels sit over the line and leaders on a well halo, under the marks. */}
      <g className="fill-ink-soft stroke-well font-sans text-xs" style={CORNER_HALO}>
        {cornerText(corners.top, LANE + 14, "top")}
        {cornerText(corners.bottom, LANE + plotH - 6, "bottom")}
      </g>
      <g data-layer="marks">
        {marks.map((m) => {
          const serious = isSerious(m.judgement);
          // The opponent's marks: a pencil hairline with no halo, so red means "your mistake".
          if (m.mine === false) {
            return (
              <line
                key={m.ply}
                data-mark="opponent"
                data-ply={m.ply}
                x1={m.x}
                x2={m.x}
                y1={m.y1}
                y2={m.y2}
                strokeWidth={1}
                strokeLinecap="round"
                className="stroke-pencil"
                style={wipeOpacity(m.x, width)}
              />
            );
          }
          const w = m.judgement === "blunder" ? 3 : serious ? 2.5 : 1.5;
          return (
            <g
              key={m.ply}
              data-mark={serious ? "pen" : "pencil"}
              data-ply={m.ply}
              style={wipeOpacity(m.x, width)}
            >
              <line
                x1={m.x}
                x2={m.x}
                y1={m.y1}
                y2={m.y2}
                strokeWidth={w + 2}
                strokeLinecap="round"
                className="stroke-well"
              />
              <line
                x1={m.x}
                x2={m.x}
                y1={m.y1}
                y2={m.y2}
                strokeWidth={w}
                strokeLinecap="round"
                className={serious ? "stroke-pen" : "stroke-pencil"}
              />
            </g>
          );
        })}
      </g>
      <g data-layer="labels">
        {labels.map((l) => (
          <text
            key={l.ply}
            data-label={l.ply}
            x={l.x}
            y={15}
            className={cn(
              "font-serif text-sm",
              !l.serious
                ? "fill-ink-faint font-normal"
                : l.mine === false
                  ? "fill-ink-soft font-semibold"
                  : "fill-ink font-semibold",
            )}
            style={{
              ...NUMERALS,
              ...wipeOpacity(l.tickX, width),
              stroke: HALO,
              strokeWidth: 4,
              strokeLinejoin: "round",
              paintOrder: "stroke",
            }}
          >
            {l.text.prefix}
            {l.text.san}
            <tspan
              className={cn(
                "tracking-glyph",
                !l.serious
                  ? "fill-ink-faint"
                  : l.mine === false
                    ? "fill-ink-soft"
                    : "fill-pen-text",
              )}
            >
              {l.text.glyph}
            </tspan>
          </text>
        ))}
      </g>
    </svg>
  );
});

interface Gesture {
  id: number;
  startX: number;
  startY: number;
  travel: number;
  touch: boolean;
  scrubbing: boolean;
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
  const [focusVisible, setFocusVisible] = useState(false);
  const gesture = useRef<Gesture | null>(null);
  const measureRef = useRef<HTMLDivElement | null>(null);
  const [cornerWidths, setCornerWidths] = useState<Record<string, number>>({});

  // The reveal's one clock: the clip's scaleX and every mark's opacity (via --wipe) follow it.
  const revealing = drawIn && !reduced;
  const progress = useMotionValue(revealing ? 0 : 1);
  const ready = width > 0;
  useEffect(() => {
    if (!ready) return;
    if (!revealing) {
      progress.jump(1);
      return;
    }
    if (progress.get() >= 1) return;
    const controls = animate(progress, 1, { duration: 2 * dur.reveal, ease: ease.pen });
    return () => controls.stop();
  }, [ready, revealing, progress]);
  /** M7: input during the reveal finishes it over `dur.quick`. */
  const finishReveal = () => {
    if (progress.get() < 1) animate(progress, 1, { duration: dur.quick, ease: ease.out });
  };

  const count = points.length;
  const first = points[0]?.ply ?? 1;
  const last = points[count - 1]?.ply ?? 1;
  const narrow = width > 0 && width < GRAPH_NARROW;
  const plotH = height ?? (narrow ? PLOT_HEIGHT.narrow : PLOT_HEIGHT.wide);
  const innerW = Math.max(0, width - 2 * PAD);

  // Scales and paths, built once per data and size. x maps by index, so plies need not start at 1.
  const geo = useMemo(() => {
    const xs = points.map((_, i) => PAD + xForPly(i + 1, innerW, count));
    const ys = points.map((p) => LANE + yForWin(p.winWhite, plotH));
    const baseline = LANE + yForWin(50, plotH);
    const index = new Map(points.map((p, i) => [p.ply, i]));
    const at = (_: EvalGraphPoint, i: number) => xs[i] ?? 0;
    const yAt = (_: EvalGraphPoint, i: number) => ys[i] ?? baseline;
    const linePath =
      Shape.line<EvalGraphPoint>({ x: at, y: yAt, curve: Curve.curveMonotoneX })(points) ?? "";
    const areaPath =
      Shape.area<EvalGraphPoint>({ x: at, y0: baseline, y1: yAt, curve: Curve.curveMonotoneX })(
        points,
      ) ?? "";
    return { xs, ys, baseline, index, linePath, areaPath };
  }, [points, innerW, count, plotH]);

  // Ticks, clamped inside the plot: blunders longest, inaccuracies shortest.
  const marks = useMemo<Mark[]>(
    () =>
      points.flatMap((p, i) => {
        if (!p.judgement) return [];
        const y = geo.ys[i] ?? 0;
        const half = p.judgement === "blunder" ? 9 : p.judgement === "mistake" ? 7 : 5;
        return [
          {
            ply: p.ply,
            x: geo.xs[i] ?? 0,
            y1: Math.max(LANE + 2, y - half),
            y2: Math.min(LANE + plotH - 2, y + half),
            judgement: p.judgement,
            mine: playerColor ? sideOf(p.ply).toLowerCase() === playerColor : null,
          },
        ];
      }),
    [points, geo, plotH, playerColor],
  );

  const labels = useMemo(() => {
    if (width <= 0) return [];
    const input: LabelMark[] = marks.map((m) => ({
      ply: m.ply,
      san: points[geo.index.get(m.ply) ?? 0]?.san ?? "",
      judgement: m.judgement,
      x: m.x,
      top: m.y1,
      mine: m.mine,
    }));
    return placeLabels(input, width, { narrow, wide: width >= LABEL_WIDE });
  }, [marks, points, geo, width, narrow]);

  const corners = useMemo(() => {
    const text = (side: "white" | "black") =>
      `${side === "white" ? "White" : "Black"}${playerColor === side ? " (you)" : ""}`;
    const top = text("white");
    const bottom = text("black");
    const line = geo.xs.map((x, i) => [x, geo.ys[i] ?? 0] as const);
    const measure = (t: string) => cornerWidths[t] ?? cornerWidth(t);
    const sides = placeCorners({ width, plotH, top, bottom, labels, ticks: marks, line, measure });
    return {
      top: { text: top, side: sides.top, width: measure(top) },
      bottom: { text: bottom, side: sides.bottom, width: measure(bottom) },
    };
  }, [playerColor, width, plotH, labels, marks, geo, cornerWidths]);

  const phaseLines = useMemo<Phase[]>(() => {
    let end = -Infinity;
    return [
      { ply: phases?.middlegameStartPly, name: "Middlegame" },
      { ply: phases?.endgameStartPly, name: "Endgame" },
    ].flatMap(({ ply, name }) => {
      const i = ply === undefined ? undefined : geo.index.get(ply);
      if (i === undefined || i <= 0) return [];
      const x = ((geo.xs[i - 1] ?? 0) + (geo.xs[i] ?? 0)) / 2;
      const w = cornerWidth(name);
      let labelX = Math.max(x + 4, end + 8);
      if (labelX + w > width) labelX = Math.max(end + 8, width - w);
      end = labelX + w;
      return [{ name, x, labelX }];
    });
  }, [phases?.middlegameStartPly, phases?.endgameStartPly, geo, width]);

  const describe = useMemo(() => summary(points, playerColor), [points, playerColor]);

  // Corner labels are measured in their real font (12px sans) once fonts are ready; until then,
  // and where canvas is unavailable, an estimate stands in.
  const measured = width > 0;
  useEffect(() => {
    const node = measureRef.current;
    if (!measured || !node) return;
    let live = true;
    const run = () => {
      const ctx = document.createElement("canvas").getContext?.("2d");
      if (!ctx || !live) return;
      const family = getComputedStyle(node).getPropertyValue("--font-sans").trim() || "sans-serif";
      ctx.font = `12px ${family}`;
      const texts = ["White", "Black", "White (you)", "Black (you)"];
      setCornerWidths(
        Object.fromEntries(texts.map((t) => [t, Math.ceil(ctx.measureText(t).width)])),
      );
    };
    try {
      run();
      void document.fonts?.ready.then(run);
    } catch {
      // no canvas (tests, old engines): keep the estimate
    }
    return () => {
      live = false;
    };
  }, [measured]);

  const atStart = currentPly === 0;
  const active = atStart ? undefined : (currentPly ?? ownPly);
  const activeIndex = active === undefined ? undefined : geo.index.get(active);

  const select = useCallback(
    (ply: number) => {
      if (ply === (currentPly ?? ownPly)) return;
      if (currentPly === undefined) setOwnPly(ply);
      onSelectPly?.(ply);
    },
    [currentPly, ownPly, onSelectPly],
  );

  const boxClass =
    height === undefined
      ? "h-(--eval-graph-height) @graph:h-(--eval-graph-height-wide)"
      : undefined;
  const boxStyle = height === undefined ? undefined : { height: LANE + height + AXIS };

  if (count === 0) {
    return (
      <figure className={cn("@container m-0", className)} aria-label={label}>
        <div
          className={cn(
            "flex items-center justify-center rounded-board bg-well text-sm text-ink-soft",
            boxClass,
          )}
          style={boxStyle}
        >
          No moves to graph yet
        </div>
      </figure>
    );
  }

  const valueIndex = activeIndex ?? count - 1;
  const valuePoint = points[valueIndex];

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const deltas: Record<string, number> = {
      ArrowRight: 1,
      ArrowUp: 1,
      ArrowLeft: -1,
      ArrowDown: -1,
      PageUp: 10,
      PageDown: -10,
    };
    let next: number | undefined;
    if (e.key === "Home") next = 0;
    else if (e.key === "End") next = count - 1;
    else if (deltas[e.key] !== undefined) {
      const delta = deltas[e.key] ?? 0;
      // From the start position only a step forward moves, to the first move.
      if (atStart) next = delta > 0 ? 0 : undefined;
      else next = Math.min(count - 1, Math.max(0, valueIndex + delta));
      if (next === undefined) {
        e.preventDefault();
        return;
      }
    }
    if (next === undefined) return;
    e.preventDefault();
    finishReveal();
    setFocusVisible(true);
    const ply = points[next]?.ply;
    if (ply !== undefined) select(ply);
  };

  const plyFromEvent = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const i = plyForX(e.clientX - rect.left - PAD, innerW, count) - 1;
    return points[i]?.ply ?? first;
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || e.isPrimary === false) return;
    finishReveal();
    const touch = e.pointerType !== "mouse";
    gesture.current = {
      id: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      travel: 0,
      touch,
      scrubbing: !touch,
    };
    if (!touch) {
      e.currentTarget.setPointerCapture?.(e.pointerId);
      select(plyFromEvent(e));
    }
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (e.isPrimary === false) return;
    const g = gesture.current;
    const ply = plyFromEvent(e);
    if (g && g.id === e.pointerId) {
      g.travel = Math.max(g.travel, Math.hypot(e.clientX - g.startX, e.clientY - g.startY));
      // A touch only scrubs after 6px of horizontal travel, so a vertical scroll never selects.
      if (!g.scrubbing && Math.abs(e.clientX - g.startX) > SCRUB_START) {
        g.scrubbing = true;
        e.currentTarget.setPointerCapture?.(e.pointerId);
      }
      if (g.scrubbing) {
        setHoverPly(ply);
        select(ply);
      }
      return;
    }
    if (e.pointerType === "mouse") {
      finishReveal();
      setHoverPly(ply);
    }
  };
  const onPointerUp = (e: PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    if (!g || g.id !== e.pointerId) return;
    gesture.current = null;
    const travel = Math.max(g.travel, Math.hypot(e.clientX - g.startX, e.clientY - g.startY));
    if (g.touch && !g.scrubbing && travel <= TAP_SLOP) select(plyFromEvent(e));
    if (e.pointerType !== "mouse") setHoverPly(null);
  };
  const onPointerCancel = () => {
    gesture.current = null;
    setHoverPly(null);
  };

  const shownIndex =
    hoverPly !== null ? geo.index.get(hoverPly) : focusVisible && !atStart ? valueIndex : undefined;
  const shownPoint = shownIndex === undefined ? undefined : points[shownIndex];
  const laneReadout = narrow && shownPoint !== undefined;
  const cursorSpring = reduced ? { duration: 0 } : spring.swing;
  const cursorX = activeIndex === undefined ? 0 : (geo.xs[activeIndex] ?? 0);

  return (
    <figure ref={measureRef} className={cn("@container m-0", className)}>
      <motion.div
        ref={parentRef}
        data-readout={laneReadout ? "" : undefined}
        className={cn("group relative w-full", boxClass)}
        style={{ ...boxStyle, "--wipe": progress } as never}
      >
        {width > 0 && (
          <StaticLayer
            width={width}
            plotH={plotH}
            linePath={geo.linePath}
            areaPath={geo.areaPath}
            baseline={geo.baseline}
            marks={marks}
            labels={labels}
            phases={phaseLines}
            corners={corners}
            progress={progress}
            reveal={revealing}
            clipId={`${id}-clip`}
          />
        )}
        {width > 0 && (
          <svg
            width={width}
            height={LANE + plotH + AXIS}
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 block"
          >
            {shownIndex !== undefined && shownIndex !== activeIndex && (
              <line
                data-hover
                x1={geo.xs[shownIndex]}
                x2={geo.xs[shownIndex]}
                y1={LANE}
                y2={LANE + plotH}
                strokeWidth={1}
                className="stroke-ink-faint"
              />
            )}
            {activeIndex !== undefined && (
              <g data-cursor style={wipeOpacity(cursorX, width)}>
                <motion.line
                  data-cursor-rule
                  x1={0}
                  x2={0}
                  y1={LANE}
                  y2={LANE + plotH}
                  strokeWidth={1.5}
                  className="stroke-ink"
                  initial={false}
                  animate={{ x: cursorX }}
                  transition={cursorSpring}
                />
                <motion.circle
                  data-cursor-dot
                  cx={0}
                  cy={0}
                  r={4}
                  strokeWidth={2}
                  className="fill-ink stroke-well"
                  initial={false}
                  animate={{ x: cursorX, y: geo.ys[activeIndex] ?? 0 }}
                  transition={cursorSpring}
                />
              </g>
            )}
          </svg>
        )}
        <div
          role="slider"
          tabIndex={0}
          aria-label={label}
          aria-valuemin={first}
          aria-valuemax={last}
          aria-valuenow={atStart ? first : (valuePoint?.ply ?? last)}
          aria-valuetext={
            atStart ? "Start position, before move 1" : valuePoint ? plyText(valuePoint) : undefined
          }
          aria-describedby={`${id}-desc`}
          className="absolute inset-x-0 cursor-crosshair touch-pan-y touch-pinch-zoom rounded-board focus-visible:outline-solid focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          style={{ top: LANE, height: plotH }}
          onKeyDown={onKeyDown}
          onFocus={(e) => {
            let visible = false;
            try {
              visible = e.currentTarget.matches(":focus-visible");
            } catch {
              // selector unsupported: wait for a key press
            }
            setFocusVisible(visible);
          }}
          onBlur={() => setFocusVisible(false)}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerCancel}
          onPointerLeave={(e) => {
            if (e.pointerType === "mouse") setHoverPly(null);
          }}
        />
        {shownPoint &&
          width > 0 &&
          (narrow ? (
            <LaneReadout point={shownPoint} top={LANE + plotH} />
          ) : (
            <Readout
              point={shownPoint}
              x={geo.xs[shownIndex ?? 0] ?? 0}
              lineY={geo.ys[shownIndex ?? 0] ?? 0}
              xs={geo.xs}
              ys={geo.ys}
              plotH={plotH}
              width={width}
              corners={corners}
              cursorX={activeIndex === undefined ? undefined : cursorX}
            />
          ))}
      </motion.div>
      <p id={`${id}-desc`} className="sr-only">
        {describe}
      </p>
    </figure>
  );
}

function Notation({ point, size = "sm" }: { point: EvalGraphPoint; size?: "sm" | "xs" }) {
  const glyph = point.judgement ? GLYPH[point.judgement] : "";
  return (
    <span
      className={cn(
        "font-serif font-semibold text-ink",
        size === "sm" ? "text-sm" : "text-xs leading-none",
      )}
      style={NUMERALS}
    >
      {movePrefix(point.ply)}
      {point.san}
      <span
        className={cn(
          "tracking-glyph",
          isSerious(point.judgement) ? "text-pen-text" : "text-ink-soft",
        )}
      >
        {glyph}
      </span>
    </span>
  );
}

const detail = (p: EvalGraphPoint) =>
  `White ${Math.round(p.winWhite)}%${p.judgement ? `, ${p.judgement}` : ""}`;

/** From 496px: a popover placed by `readoutPlacement`, away from the cursor and corner labels. */
function Readout({
  point,
  corners,
  ...geometry
}: {
  point: EvalGraphPoint;
  x: number;
  lineY: number;
  cursorX: number | undefined;
  xs: number[];
  ys: number[];
  plotH: number;
  width: number;
  corners: { top: Corner; bottom: Corner };
}) {
  const { left, top } = readoutPlacement({ ...geometry, corners });
  return (
    <div
      aria-hidden="true"
      data-readout-popover
      className="pointer-events-none absolute flex flex-col rounded-field bg-sheet px-3 py-1.5 shadow-lift"
      style={{ left, top, width: READOUT_W, height: READOUT_H }}
    >
      <Notation point={point} />
      <span className="font-sans text-xs text-ink-soft">{detail(point)}</span>
    </div>
  );
}

/**
 * Below 496px: one line in the phase lane (which hides the phase labels meanwhile), 6px under
 * the plot so it clears the 2px focus ring and its 2px offset.
 */
function LaneReadout({ point, top }: { point: EvalGraphPoint; top: number }) {
  return (
    <div
      aria-hidden="true"
      data-readout-lane
      className="pointer-events-none absolute inset-x-0 flex items-end gap-2 leading-none"
      style={{ top: top + 6, height: AXIS - 6 }}
    >
      <Notation point={point} size="xs" />
      <span className="font-sans text-xs leading-none text-ink-soft">{detail(point)}</span>
    </div>
  );
}
