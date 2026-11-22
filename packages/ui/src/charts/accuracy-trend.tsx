"use client";

import { Responsive, Shape } from "@visx/visx";
import {
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  memo,
  type PointerEvent,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { CaretLeftIcon, CaretRightIcon, Icon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
import { Popover, PopoverContent, PopoverTrigger } from "../primitives/index.js";
import {
  ACCURACY_TICKS,
  DOT_R,
  dateTicks,
  HIT_RADIUS,
  isDense,
  nearestIndex,
  runs,
  TREND_HEIGHT,
  TREND_LEFT,
  TREND_PLOT_H,
  TREND_TOP,
  timeScale,
  trendSummary,
  yForAccuracy,
} from "./scales.js";

export type TrendResult = "win" | "draw" | "loss" | "unfinished";

export interface AccuracyTrendPoint {
  id: string;
  /** ISO date-time the game was played. */
  date: string;
  /** The player's accuracy in the game, 0 to 100. A non-finite value is left out, and counted. */
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
  /** The 80% band's ends. Null when the window has no spread: the mean is drawn, no band. */
  low: number | null;
  high: number | null;
}

export interface AccuracyTrendProps {
  /** The games, in any order: they are drawn and walked in date order. */
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

/** True on a touch screen, where taps reach further. The server render assumes fine. */
function useCoarsePointer(): boolean {
  return useSyncExternalStore(subscribeCoarse, coarseSnapshot, () => false);
}

const finite = (v: number | null): v is number => v !== null && Number.isFinite(v);
const clamp100 = (v: number) => Math.min(100, Math.max(0, v));

/**
 * One game's mark, by result (R11): filled, half-filled, a ring, or a cross. `soft` steps every
 * tone down one (dense plots), so the mean line stays the strongest mark.
 */
function Dot({ result, r, soft = false }: { result: TrendResult; r: number; soft?: boolean }) {
  const halo = <circle r={r + 1.5} style={{ fill: SURFACE }} />;
  const ink = soft ? "fill-ink-soft" : "fill-ink";
  const inkStroke = soft ? "stroke-ink-soft" : "stroke-ink";
  if (result === "win") {
    return (
      <>
        {halo}
        <circle data-part="fill" r={r} className={ink} />
      </>
    );
  }
  if (result === "draw") {
    // The left half filled: a vertical split reads at 3px where a horizontal one would not.
    return (
      <>
        {halo}
        <path data-part="half" d={`M0,${-r}A${r},${r} 0 0 0 0,${r}Z`} className={ink} />
        <circle data-part="ring" r={r - 0.6} strokeWidth={1.2} fill="none" className={inkStroke} />
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
          className={soft ? "stroke-rule-strong" : "stroke-ink-soft"}
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

interface PlacedDot extends AccuracyTrendPoint {
  time: Date;
  x: number;
  y: number;
}

/** The dots. Memoised: keys, hovers and the popover never re-render 500 marks. */
const DotLayer = memo(function DotLayer({
  dots,
  r,
  soft,
  width,
}: {
  dots: PlacedDot[];
  r: number;
  soft: boolean;
  width: number;
}) {
  const fade = (x: number): CSSProperties => ({
    opacity:
      `clamp(0, calc((var(--draw, 1) * ${width + FADE} - ${x.toFixed(1)}) / ${FADE}), 1)` as unknown as number,
  });
  return (
    <g data-layer="dots">
      {dots.map((d) => (
        <g
          key={d.id}
          data-id={d.id}
          data-result={d.result}
          data-cx={d.x.toFixed(2)}
          data-cy={d.y.toFixed(2)}
          transform={`translate(${d.x.toFixed(2)} ${d.y.toFixed(2)})`}
          style={fade(d.x)}
        >
          <Dot result={d.result} r={r} soft={soft} />
        </g>
      ))}
    </g>
  );
});

/**
 * A game's keyboard and screen-reader handle, centred on its dot. Pointer hits go to the one
 * overlay instead (nearest dot wins), so these never fight over a tap. Memoised so moving the
 * roving tab stop re-renders only the two buttons whose `tabbable` changed.
 */
const DotButton = memo(function DotButton({
  dot,
  index,
  id,
  tabbable,
  setRef,
  onSelect,
  onKey,
  onFocusDot,
}: {
  dot: PlacedDot;
  index: number;
  id: string;
  tabbable: boolean;
  setRef: (index: number, el: HTMLElement | null) => void;
  onSelect: (index: number) => void;
  onKey: (event: KeyboardEvent<HTMLButtonElement>, index: number) => void;
  onFocusDot: (index: number) => void;
}) {
  const ref = useCallback((el: HTMLElement | null) => setRef(index, el), [setRef, index]);
  return (
    <PopoverTrigger
      id={id}
      ref={ref}
      aria-label={dot.caption}
      tabIndex={tabbable ? 0 : -1}
      onClick={() => onSelect(index)}
      onFocus={() => onFocusDot(index)}
      onKeyDown={(e: KeyboardEvent<HTMLButtonElement>) => onKey(e, index)}
      className={cn(
        "pointer-events-none absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full",
        "outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink",
      )}
      style={{ left: dot.x, top: dot.y }}
    />
  );
});

const stepButton =
  "inline-flex size-8 items-center justify-center rounded-field text-ink-soft hover:bg-well " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink " +
  "disabled:text-ink-faint disabled:hover:bg-transparent";

/**
 * The player's accuracy per game over time (R11). Each dot is one game, shaped by its result;
 * the line is the rolling mean over its 80% band. The line breaks where there is no rolling
 * entry, and the band is left out wherever an end is null, so nothing is bridged or invented.
 *
 * Pointer and touch: one overlay picks the dot nearest the tap (within 12px, 22px on touch) and
 * opens its caption, with Previous and Next to step through a dense cluster. Keyboard: the dots
 * are one tab stop; arrow keys walk the games in date order, Home and End jump to the ends.
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
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const clipId = `at${uid}`;
  const { parentRef, width } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });

  const sorted = useMemo(
    () =>
      points
        .map((p) => ({ ...p, time: new Date(p.date) }))
        .filter((p) => Number.isFinite(p.time.getTime()) && Number.isFinite(p.accuracy))
        .sort((a, b) => a.time.getTime() - b.time.getTime()),
    [points],
  );
  const omitted = points.length - sorted.length;
  const enough = sorted.length >= 2;

  // One clock: a single clip sweeps left to right over the band and line, and each dot fades in
  // as the sweep reaches it. Under reduced motion it starts, and stays, at the end. The clip's
  // `width` attribute is driven directly (not a CSS transform), which every engine repaints.
  const revealing = drawIn && !reduced && enough;
  const progress = useMotionValue(revealing ? 0 : 1);
  const clipRect = useRef<SVGRectElement | null>(null);
  const widthRef = useRef(width);
  widthRef.current = width;
  const ready = width > 0;
  useEffect(
    () =>
      progress.on("change", (v) => {
        clipRect.current?.setAttribute("width", (v * widthRef.current).toFixed(1));
      }),
    [progress],
  );
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

  const geo = useMemo(() => {
    if (!ready || !enough) return null;
    const x = timeScale(
      sorted.map((p) => p.time),
      width,
    );
    const dots: PlacedDot[] = sorted.map((p) => ({
      ...p,
      x: x(p.time),
      y: yForAccuracy(clamp100(p.accuracy)),
    }));
    const byId = new Map(rolling.map((r) => [r.id, r]));
    const at = (d: PlacedDot) => byId.get(d.id);
    type Row = { d: PlacedDot; r: AccuracyTrendRolling };
    const rows = (run: PlacedDot[]): Row[] =>
      run.map((d) => ({ d, r: at(d) as AccuracyTrendRolling }));
    // The band and the mean break separately: a null interval keeps its mean, without a band.
    const bands = runs(dots, (d) => {
      const r = at(d);
      return !!r && finite(r.low) && finite(r.high) && r.high > r.low;
    }).map((run) => ({
      key: run[0]?.id ?? "",
      d:
        Shape.area<Row>({
          x: (row) => row.d.x,
          y0: (row) => yForAccuracy(clamp100(row.r.low as number)),
          y1: (row) => yForAccuracy(clamp100(row.r.high as number)),
        })(rows(run)) ?? "",
    }));
    const means = runs(dots, (d) => {
      const r = at(d);
      return !!r && Number.isFinite(r.mean);
    }).map((run) => ({
      key: run[0]?.id ?? "",
      d:
        Shape.line<Row>({
          x: (row) => row.d.x,
          y: (row) => yForAccuracy(clamp100(row.r.mean)),
        })(rows(run)) ?? "",
    }));
    return { dots, bands, means, ticks: dateTicks(x, width) };
  }, [ready, enough, sorted, rolling, width]);

  const summary = useMemo(() => trendSummary(sorted, rolling), [sorted, rolling]);

  // Selection: which game the popover shows, and the roving tab stop.
  const [active, setActive] = useState<number | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [open, setOpen] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const count = sorted.length;
  const current = Math.min(active ?? count - 1, count - 1);
  const buttons = useRef<(HTMLElement | null)[]>([]);
  const idFor = (i: number) => `${uid}-dot-${i}`;

  const setRef = useCallback((i: number, el: HTMLElement | null) => {
    buttons.current[i] = el;
  }, []);
  const onSelect = useCallback((i: number) => {
    setSelected(i);
    setActive(i);
  }, []);
  const countRef = useRef(count);
  countRef.current = count;
  const onKey = useCallback((event: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const last = countRef.current - 1;
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
  }, []);
  const onFocusDot = useCallback((i: number) => setActive(i), []);

  if (!enough) {
    return (
      <div className={cn("flex flex-col gap-2", className)}>
        <div className="flex h-24 items-center justify-center rounded-field bg-well px-4 text-center">
          <p className="m-0 text-sm text-ink-soft">Not enough games yet</p>
        </div>
        {omitted > 0 && <OmittedNote count={omitted} />}
      </div>
    );
  }

  const dense = ready && isDense(count, width);
  const r = !dense && coarse ? DOT_R.coarse : DOT_R.fine;
  const plotBottom = TREND_TOP + TREND_PLOT_H;
  const counts = LEGEND.map((l) => ({
    ...l,
    n: sorted.filter((p) => p.result === l.result).length,
  })).filter((l) => l.n > 0);

  const pick = (event: MouseEvent<HTMLElement> | PointerEvent<HTMLElement>, radius: number) => {
    if (!geo) return null;
    const box = event.currentTarget.getBoundingClientRect();
    return nearestIndex(geo.dots, event.clientX - box.left, event.clientY - box.top, radius);
  };
  const onTap = (event: MouseEvent<HTMLElement>) => {
    const i = pick(event, coarse ? HIT_RADIUS.coarse : HIT_RADIUS.fine);
    if (i === null) return;
    setSelected(i);
    setActive(i);
    setOpen(true);
  };
  const onHover = (event: PointerEvent<HTMLElement>) => {
    if (event.pointerType !== "mouse") return;
    const i = pick(event, HIT_RADIUS.fine);
    if (i !== hover) setHover(i);
  };
  const step = (delta: number) => {
    if (selected === null) return;
    const next = Math.min(count - 1, Math.max(0, selected + delta));
    setSelected(next);
    setActive(next);
  };
  const shown = open && selected !== null ? geo?.dots[selected] : undefined;
  const hovered = hover !== null && hover !== selected ? geo?.dots[hover] : undefined;

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <p className="sr-only">{summary}</p>
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
                    ref={clipRect}
                    x={0}
                    y={0}
                    width={(progress.get() * width).toFixed(0)}
                    height={TREND_HEIGHT}
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
                {geo.bands.map((b) => (
                  <path key={b.key} data-part="band" d={b.d} className="fill-pencil/45" />
                ))}
              </g>
              <DotLayer dots={geo.dots} r={r} soft={dense} width={width} />
              <g clipPath={`url(#${clipId})`}>
                {geo.means.map((m) => (
                  <g key={m.key}>
                    <path
                      d={m.d}
                      fill="none"
                      strokeWidth={5}
                      strokeLinejoin="round"
                      strokeLinecap="round"
                      style={{ stroke: SURFACE }}
                    />
                    <path
                      data-part="mean"
                      d={m.d}
                      fill="none"
                      strokeWidth={2}
                      strokeLinejoin="round"
                      strokeLinecap="round"
                      className="stroke-ink"
                    />
                  </g>
                ))}
              </g>
              {hovered && (
                <circle
                  data-part="hover"
                  cx={hovered.x}
                  cy={hovered.y}
                  r={r + 3.5}
                  fill="none"
                  strokeWidth={1}
                  className="stroke-ink-soft"
                />
              )}
              {shown && (
                <circle
                  data-part="selected"
                  cx={shown.x}
                  cy={shown.y}
                  r={r + 4}
                  fill="none"
                  strokeWidth={2}
                  className="stroke-ink"
                />
              )}
            </motion.svg>
            {/* The one pointer target: the nearest dot to a tap or click wins. */}
            <div
              data-part="hit"
              aria-hidden="true"
              className={cn("absolute inset-0", hovered && "cursor-pointer")}
              onClick={onTap}
              onPointerMove={onHover}
              onPointerLeave={() => setHover(null)}
            />
            <Popover
              open={open}
              onOpenChange={setOpen}
              triggerId={selected === null ? null : idFor(selected)}
            >
              <div className="pointer-events-none absolute inset-0">
                {geo.dots.map((d, i) => (
                  <DotButton
                    key={d.id}
                    dot={d}
                    index={i}
                    id={idFor(i)}
                    tabbable={i === current}
                    setRef={setRef}
                    onSelect={onSelect}
                    onKey={onKey}
                    onFocusDot={onFocusDot}
                  />
                ))}
              </div>
              {shown && selected !== null && (
                <PopoverContent side="top" align="center" className="w-64">
                  <p className="m-0 text-sm text-ink">{shown.caption}</p>
                  <div className="flex items-center justify-between gap-2">
                    <a
                      href={shown.href}
                      className="rounded-field text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
                    >
                      Review game
                    </a>
                    <div className="flex gap-3">
                      <button
                        type="button"
                        aria-label="Previous game"
                        disabled={selected === 0}
                        onClick={() => step(-1)}
                        className={stepButton}
                      >
                        <Icon icon={CaretLeftIcon} size={16} />
                      </button>
                      <button
                        type="button"
                        aria-label="Next game"
                        disabled={selected === count - 1}
                        onClick={() => step(1)}
                        className={stepButton}
                      >
                        <Icon icon={CaretRightIcon} size={16} />
                      </button>
                    </div>
                  </div>
                </PopoverContent>
              )}
            </Popover>
          </>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-sm text-ink-soft">
        {counts.map((c) => (
          <span key={c.result} className="flex items-center gap-2">
            <svg aria-hidden="true" width={12} height={12} className="shrink-0 overflow-visible">
              <g transform="translate(6 6)">
                <Dot result={c.result} r={4} soft={dense} />
              </g>
            </svg>
            <span className="tabular-nums">
              {c.text} {c.n}
            </span>
          </span>
        ))}
        {geo && geo.means.length > 0 && (
          <span className="flex items-center gap-2">
            <svg aria-hidden="true" width={18} height={12} className="shrink-0">
              <rect x={0} y={2} width={18} height={8} rx={1} className="fill-pencil/45" />
              <line x1={0} x2={18} y1={6} y2={6} strokeWidth={2} className="stroke-ink" />
            </svg>
            <span>Rolling average, 80% band</span>
          </span>
        )}
        {omitted > 0 && <OmittedNote count={omitted} />}
      </div>
    </div>
  );
}

function OmittedNote({ count }: { count: number }) {
  return (
    <span className="text-sm text-ink-faint">
      {count === 1
        ? "1 game has no accuracy and isn't shown."
        : `${count} games have no accuracy and aren't shown.`}
    </span>
  );
}
