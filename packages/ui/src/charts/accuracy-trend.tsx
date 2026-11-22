"use client";

import { Responsive, Shape } from "@visx/visx";
import {
  type CSSProperties,
  type KeyboardEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
import { Popover, PopoverContent, PopoverTrigger } from "../primitives/index.js";
import { touchTarget } from "../primitives/touch.js";
import {
  ACCURACY_TICKS,
  DOT_R,
  dateTicks,
  runs,
  TREND_HEIGHT,
  TREND_LEFT,
  TREND_PLOT_H,
  TREND_TOP,
  timeScale,
  yForAccuracy,
} from "./scales.js";

export type TrendResult = "win" | "draw" | "loss" | "unfinished";

export interface AccuracyTrendPoint {
  id: string;
  /** ISO date-time the game was played. */
  date: string;
  /** The player's accuracy in the game, 0 to 100. */
  accuracy: number;
  result: TrendResult;
  /** One line about the game, shown when its dot is opened and used as its accessible name. */
  caption: string;
  /** The game's review page. */
  href: string;
}

export interface AccuracyTrendRolling {
  /** The point (game) this window ends on. */
  id: string;
  mean: number;
  low: number;
  high: number;
}

export interface AccuracyTrendProps {
  points: AccuracyTrendPoint[];
  /** The rolling mean and its band, one entry per game that has a full enough window. */
  rolling: AccuracyTrendRolling[];
  /** Accessible name of the chart. */
  label: string;
  /** Sweep the chart in from the left on mount over 2 × `dur.reveal` (not under reduced motion). */
  drawIn?: boolean;
  className?: string;
}

const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];
/** Every mark's halo reads the host's surface (the `--stat-surface` pattern). */
const SURFACE = "var(--chart-surface, var(--color-sheet))";
/** Dots fade in over this many px as the sweep passes them. */
const FADE = 16;
const COARSE = "(pointer: coarse)";

function subscribeCoarse(onChange: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  const media = window.matchMedia(COARSE);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

const coarseSnapshot = () =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(COARSE).matches
    : false;

/** True on a touch screen, where dots grow to stay findable. The server render assumes fine. */
function useCoarsePointer(): boolean {
  return useSyncExternalStore(subscribeCoarse, coarseSnapshot, () => false);
}

const clamp100 = (v: number) => (Number.isFinite(v) ? Math.min(100, Math.max(0, v)) : 0);

/** One game's mark, by result (R11): filled, half-filled, a ring, or a cross. */
function Dot({ result, r }: { result: TrendResult; r: number }) {
  const halo = <circle r={r + 1.5} style={{ fill: SURFACE }} />;
  if (result === "win") {
    return (
      <>
        {halo}
        <circle data-part="fill" r={r} className="fill-ink" />
      </>
    );
  }
  if (result === "draw") {
    // The left half filled: a vertical split reads at 3px where a horizontal one would not.
    return (
      <>
        {halo}
        <path data-part="half" d={`M0,${-r}A${r},${r} 0 0 0 0,${r}Z`} className="fill-ink" />
        <circle data-part="ring" r={r - 0.6} strokeWidth={1.2} fill="none" className="stroke-ink" />
      </>
    );
  }
  if (result === "loss") {
    return (
      <>
        {halo}
        <circle
          data-part="ring"
          r={r - 0.6}
          strokeWidth={1.2}
          fill="none"
          className="stroke-ink-soft"
        />
      </>
    );
  }
  const a = r * 0.9;
  return (
    <>
      {halo}
      <path
        data-part="cross"
        d={`M${-a},${-a}L${a},${a}M${-a},${a}L${a},${-a}`}
        strokeWidth={1.5}
        strokeLinecap="round"
        className="stroke-pencil"
      />
    </>
  );
}

const LEGEND: { result: TrendResult; text: string }[] = [
  { result: "win", text: "Win" },
  { result: "draw", text: "Draw" },
  { result: "loss", text: "Loss" },
  { result: "unfinished", text: "Unfinished" },
];

/**
 * The player's accuracy per game over time (R11). Each dot is one game, shaped by its result;
 * the line is the rolling mean over its 80% band, broken wherever the window is too thin to
 * state one. Every dot opens its caption and a "Review game" link. The dots are one tab stop:
 * arrow keys walk the games in date order, Home and End jump to the ends.
 */
export function AccuracyTrend({
  points,
  rolling,
  label,
  drawIn = false,
  className,
}: AccuracyTrendProps) {
  const reduced = useReducedMotion();
  const coarse = useCoarsePointer();
  const clipId = `at${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const { parentRef, width } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });

  const sorted = useMemo(
    () =>
      points
        .map((p) => ({ ...p, time: new Date(p.date) }))
        .filter((p) => Number.isFinite(p.time.getTime()))
        .sort((a, b) => a.time.getTime() - b.time.getTime()),
    [points],
  );
  const enough = sorted.length >= 2;

  // One clock: a single clip sweeps left to right over the band and line, and each dot fades in
  // as the sweep reaches it. Under reduced motion it starts, and stays, at the end.
  const revealing = drawIn && !reduced && enough;
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

  const [active, setActive] = useState<number | null>(null);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  // One stable ref callback per dot: a fresh callback each render would re-register every
  // trigger with the popover on every render.
  const refs = useRef(new Map<number, (el: HTMLElement | null) => void>());
  const refFor = (i: number) => {
    let fn = refs.current.get(i);
    if (!fn) {
      fn = (el) => {
        buttons.current[i] = el as HTMLButtonElement | null;
      };
      refs.current.set(i, fn);
    }
    return fn;
  };
  const current = Math.min(active ?? sorted.length - 1, sorted.length - 1);

  const geo = useMemo(() => {
    if (!ready || !enough) return null;
    const x = timeScale(
      sorted.map((p) => p.time),
      width,
    );
    const dots = sorted.map((p) => ({
      ...p,
      x: x(p.time),
      y: yForAccuracy(clamp100(p.accuracy)),
    }));
    const byId = new Map(rolling.map((r) => [r.id, r]));
    const segments = runs(dots, (d) => byId.has(d.id)).map((run) => {
      const rows = run.map((d) => ({ d, r: byId.get(d.id) as AccuracyTrendRolling }));
      type Row = (typeof rows)[number];
      return {
        key: run[0]?.id ?? "",
        band:
          Shape.area<Row>({
            x: (row) => row.d.x,
            y0: (row) => yForAccuracy(clamp100(row.r.low)),
            y1: (row) => yForAccuracy(clamp100(row.r.high)),
          })(rows) ?? "",
        mean:
          Shape.line<Row>({
            x: (row) => row.d.x,
            y: (row) => yForAccuracy(clamp100(row.r.mean)),
          })(rows) ?? "",
      };
    });
    return { dots, segments, ticks: dateTicks(x, width) };
  }, [ready, enough, sorted, rolling, width]);

  if (!enough) {
    return (
      <div
        className={cn(
          "flex h-24 items-center justify-center rounded-field bg-well px-4 text-center",
          className,
        )}
      >
        <p className="text-sm text-ink-soft">Not enough games yet</p>
      </div>
    );
  }

  const r = coarse ? DOT_R.coarse : DOT_R.fine;
  const plotBottom = TREND_TOP + TREND_PLOT_H;
  const counts = LEGEND.map((l) => ({
    ...l,
    n: sorted.filter((p) => p.result === l.result).length,
  })).filter((l) => l.n > 0);
  const dotStyle = (x: number): CSSProperties => ({
    opacity:
      `clamp(0, calc((var(--draw, 1) * ${width + FADE} - ${x.toFixed(1)}) / ${FADE}), 1)` as unknown as number,
  });

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const last = sorted.length - 1;
    const next =
      event.key === "ArrowLeft" || event.key === "ArrowDown"
        ? Math.max(0, i - 1)
        : event.key === "ArrowRight" || event.key === "ArrowUp"
          ? Math.min(last, i + 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (next === null) return;
    event.preventDefault();
    setActive(next);
    buttons.current[next]?.focus();
  };

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <div ref={parentRef} className="relative h-52 w-full">
        {geo && (
          <>
            <motion.svg
              role="img"
              aria-label={label}
              width={width}
              height={TREND_HEIGHT}
              className="absolute inset-0 block overflow-visible"
              style={{ "--draw": progress } as never}
            >
              <defs>
                <clipPath id={clipId}>
                  <rect
                    x={0}
                    y={0}
                    width={width}
                    height={TREND_HEIGHT}
                    style={{ transform: "scaleX(var(--draw, 1))", transformOrigin: "0 0" }}
                  />
                </clipPath>
              </defs>
              {/* Gridlines are decorative (`rule`); the zero line is the axis (`rule-strong`). */}
              {ACCURACY_TICKS.map((t) => (
                <g key={t} data-tick-y={t}>
                  <line
                    x1={TREND_LEFT}
                    x2={width}
                    y1={yForAccuracy(t)}
                    y2={yForAccuracy(t)}
                    strokeWidth={1}
                    className="stroke-rule"
                  />
                  <text
                    x={TREND_LEFT - 6}
                    y={yForAccuracy(t) + 4}
                    textAnchor="end"
                    className="fill-ink-faint font-sans text-xs tabular-nums"
                  >
                    {t}%
                  </text>
                </g>
              ))}
              <line
                x1={TREND_LEFT}
                x2={width}
                y1={plotBottom}
                y2={plotBottom}
                strokeWidth={1}
                className="stroke-rule-strong"
              />
              {geo.ticks.map((t) => (
                <text
                  key={t.iso}
                  x={Math.min(Math.max(t.x, TREND_LEFT + 18), width - 24)}
                  y={plotBottom + 18}
                  textAnchor="middle"
                  className="fill-ink-faint font-sans text-xs tabular-nums"
                >
                  {t.text}
                </text>
              ))}
              <g clipPath={`url(#${clipId})`}>
                {geo.segments.map((s) => (
                  <path key={s.key} data-part="band" d={s.band} className="fill-rule-strong/25" />
                ))}
              </g>
              <g data-layer="dots">
                {geo.dots.map((d) => (
                  <g
                    key={d.id}
                    data-result={d.result}
                    data-cx={d.x.toFixed(2)}
                    transform={`translate(${d.x.toFixed(2)} ${d.y.toFixed(2)})`}
                    style={dotStyle(d.x)}
                  >
                    <Dot result={d.result} r={r} />
                  </g>
                ))}
              </g>
              <g clipPath={`url(#${clipId})`}>
                {geo.segments.map((s) => (
                  <g key={s.key}>
                    <path
                      d={s.mean}
                      fill="none"
                      strokeWidth={5}
                      strokeLinejoin="round"
                      strokeLinecap="round"
                      style={{ stroke: SURFACE }}
                    />
                    <path
                      data-part="mean"
                      d={s.mean}
                      fill="none"
                      strokeWidth={2}
                      strokeLinejoin="round"
                      strokeLinecap="round"
                      className="stroke-ink"
                    />
                  </g>
                ))}
              </g>
            </motion.svg>
            <Popover<number>>
              {({ payload }) => {
                const open = payload === undefined ? undefined : geo.dots[payload];
                return (
                  <>
                    <div className="absolute inset-0">
                      {geo.dots.map((d, i) => (
                        <PopoverTrigger<number>
                          key={d.id}
                          payload={i}
                          ref={refFor(i)}
                          aria-label={d.caption}
                          tabIndex={i === current ? 0 : -1}
                          onFocus={() => setActive(i)}
                          onKeyDown={(e: KeyboardEvent<HTMLButtonElement>) => onKeyDown(e, i)}
                          className={cn(
                            "absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full",
                            "outline-offset-2 hover:outline-1 hover:outline-rule-strong",
                            "focus-visible:outline-2 focus-visible:outline-ink",
                            "data-popup-open:outline-2 data-popup-open:outline-ink",
                            touchTarget,
                          )}
                          style={{ left: d.x, top: d.y }}
                        />
                      ))}
                    </div>
                    {open && (
                      <PopoverContent side="top" align="center" className="w-64">
                        <p className="m-0 text-sm text-ink">{open.caption}</p>
                        <a
                          href={open.href}
                          className="self-start rounded-field text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
                        >
                          Review game
                        </a>
                      </PopoverContent>
                    )}
                  </>
                );
              }}
            </Popover>
          </>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-sm text-ink-soft">
        {counts.map((c) => (
          <span key={c.result} className="flex items-center gap-2">
            <svg aria-hidden="true" width={12} height={12} className="shrink-0 overflow-visible">
              <g transform="translate(6 6)">
                <Dot result={c.result} r={4} />
              </g>
            </svg>
            <span className="tabular-nums">
              {c.text} {c.n}
            </span>
          </span>
        ))}
        {geo && geo.segments.length > 0 && (
          <span className="flex items-center gap-2">
            <svg aria-hidden="true" width={18} height={12} className="shrink-0">
              <rect x={0} y={2} width={18} height={8} rx={1} className="fill-rule-strong/25" />
              <line x1={0} x2={18} y1={6} y2={6} strokeWidth={2} className="stroke-ink" />
            </svg>
            <span>Rolling average, 80% band</span>
          </span>
        )}
      </div>
    </div>
  );
}
