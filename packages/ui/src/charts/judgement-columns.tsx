"use client";

import { Responsive } from "@visx/visx";
import { type KeyboardEvent, useId, useLayoutEffect, useRef, useState } from "react";
import { cn } from "../lib/index.js";
import { ChartPatterns, patternFill } from "./patterns.js";
import {
  CELL_GAP,
  COLUMNS_MIN_ROWS,
  cellHeight,
  columnsWidth,
  countText,
  whole,
} from "./scales.js";

export interface JudgementColumnsGame {
  id: string;
  /** The column's accessible name, for example "12 Nov 2026, win v Opponent (1520)". */
  label: string;
  href: string;
  blunders: number;
  mistakes: number;
  inaccuracies: number;
}

export interface JudgementColumnsProps {
  /** One entry per game, oldest first: the oldest is drawn on the left, the latest on the right. */
  games: JudgementColumnsGame[];
  /** Accessible name of the chart. */
  label: string;
  className?: string;
}

type Kind = "blunder" | "mistake" | "inaccuracy";

/** The widest a column gets, so a handful of games stay unit columns rather than slabs. */
const COLUMN_MAX_W = 24;
const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];
/** When the columns scroll, the oldest edge fades over this many px: there is more history. */
const EDGE_FADE = 24;

/** A move's cell. Blunders are solid pen, mistakes pen hatch, inaccuracies a pencil outline. */
function Cell({
  kind,
  x,
  y,
  w,
  h,
  prefix,
  swatch = false,
}: {
  kind: Kind;
  x: number;
  y: number;
  w: number;
  h: number;
  prefix: string;
  /** A legend swatch carries no `data-cell`, so only the chart's own cells count. */
  swatch?: boolean;
}) {
  if (kind === "blunder") {
    return (
      <rect
        data-cell={swatch ? undefined : "blunder"}
        x={x}
        y={y}
        width={w}
        height={h}
        rx={1}
        className="fill-pen"
      />
    );
  }
  // Outlined cells are inset by half the stroke, so every cell keeps the same outer box.
  return (
    <rect
      data-cell={swatch ? undefined : kind}
      x={x + 0.5}
      y={y + 0.5}
      width={Math.max(0, w - 1)}
      height={Math.max(0, h - 1)}
      rx={1}
      strokeWidth={1}
      fill={kind === "mistake" ? patternFill(prefix, "hatch-pen") : "none"}
      className={kind === "mistake" ? "stroke-pen" : "stroke-pencil"}
    />
  );
}

function Swatch({ kind, prefix }: { kind: Kind; prefix: string }) {
  return (
    <svg aria-hidden="true" width={14} height={10} className="shrink-0">
      <Cell kind={kind} x={0} y={0} w={14} h={10} prefix={prefix} swatch />
    </svg>
  );
}

/**
 * The player's own judged moves per game, one column per game and one cell per move, stacked
 * from the baseline: blunders, then mistakes, then inaccuracies. Counting cells gives the exact
 * number, so the chart needs no value axis. Red means "your mistake" here, as everywhere.
 *
 * The latest game sits at the right edge. Below 480px the columns keep a 12px least width, the
 * chart scrolls inside its own container (never the page) and opens scrolled to the latest
 * games. Each column links to its game's review and is described by its counts; the columns are
 * one tab stop, with arrow keys, Home and End between games. Cells shrink to fit a tall column
 * down to 2px; a game with more than about 33 marked moves draws taller than the usual 132px.
 */
export function JudgementColumns({ games, label, className }: JudgementColumnsProps) {
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const prefix = `jc${uid}`;
  const { parentRef, width: measured } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });
  const scroller = useRef<HTMLElement | null>(null);
  const links = useRef<(HTMLAnchorElement | null)[]>([]);
  const [active, setActive] = useState<number | null>(null);

  const svgW = columnsWidth(measured, games.length);
  const scrolls = measured > 0 && svgW > measured;

  // Open on the latest games: the right end of the scroll, on mount and whenever they change.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && games.length > 0 && svgW > 0) el.scrollLeft = el.scrollWidth;
  }, [games, svgW]);

  if (games.length === 0) {
    return (
      <div
        className={cn(
          "flex h-24 items-center justify-center rounded-field bg-well px-4 text-center",
          className,
        )}
      >
        <p className="m-0 text-sm text-ink-soft">No analysed games yet</p>
      </div>
    );
  }

  const counts = games.map((g) => ({
    game: g,
    blunders: whole(g.blunders),
    mistakes: whole(g.mistakes),
    inaccuracies: whole(g.inaccuracies),
  }));
  const tallest = Math.max(0, ...counts.map((c) => c.blunders + c.mistakes + c.inaccuracies));
  const cellH = cellHeight(tallest);
  const plotH = Math.ceil(Math.max(tallest, COLUMNS_MIN_ROWS) * (cellH + CELL_GAP));
  const baseline = plotH;
  const height = plotH + 1;
  // Each game's slot divides the whole width, oldest at the left edge (T13 critic) and the latest
  // at the right (web polish critic I2: a row that stopped at 70% looked cut off). The block in a
  // slot is at most COLUMN_MAX_W wide and centred, so a few games stay unit columns, not slabs. A
  // row wider than the container scrolls, and opens at its right end on the latest games.
  const colW = svgW / games.length;
  const offset = 0;
  const cellW = Math.max(2, Math.min(COLUMN_MAX_W, colW) - CELL_GAP);
  const total = counts.reduce(
    (t, c) => ({
      blunders: t.blunders + c.blunders,
      mistakes: t.mistakes + c.mistakes,
      inaccuracies: t.inaccuracies + c.inaccuracies,
    }),
    { blunders: 0, mistakes: 0, inaccuracies: 0 },
  );
  const current = Math.min(active ?? games.length - 1, games.length - 1);
  const descId = (i: number) => `${uid}-desc-${i}`;

  const onKeyDown = (event: KeyboardEvent<HTMLAnchorElement>, i: number) => {
    const last = games.length - 1;
    const next =
      event.key === "ArrowLeft"
        ? Math.max(0, i - 1)
        : event.key === "ArrowRight"
          ? Math.min(last, i + 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (next === null) return;
    event.preventDefault();
    setActive(next);
    links.current[next]?.focus();
  };

  return (
    <div className={cn("relative flex flex-col gap-3", className)}>
      <ChartPatterns prefix={prefix} />
      <p className="sr-only">
        {`${games.length} ${games.length === 1 ? "game" : "games"}: ${countText(total)}.`}
      </p>
      <ul className="sr-only">
        {counts.map((c, i) => (
          <li key={c.game.id} id={descId(i)}>
            {countText(c)}
          </li>
        ))}
      </ul>
      <figure
        ref={(el) => {
          parentRef(el as HTMLDivElement | null);
          scroller.current = el;
        }}
        aria-label={label}
        data-part="scroller"
        className="m-0 w-full overflow-x-auto pb-1"
        style={
          scrolls
            ? {
                maskImage: `linear-gradient(to right, transparent, black ${EDGE_FADE}px)`,
                WebkitMaskImage: `linear-gradient(to right, transparent, black ${EDGE_FADE}px)`,
              }
            : undefined
        }
      >
        <svg width={svgW} height={height} className="block overflow-visible">
          <title>{label}</title>
          {counts.map((c, i) => {
            const x = offset + i * colW;
            const cx = x + (colW - cellW) / 2;
            // Each cell's slot is its height above the baseline, and its stable key.
            const kinds = [
              ...Array<Kind>(c.blunders).fill("blunder"),
              ...Array<Kind>(c.mistakes).fill("mistake"),
              ...Array<Kind>(c.inaccuracies).fill("inaccuracy"),
            ].map((kind, slot) => ({ kind, slot }));
            return (
              <a
                key={c.game.id}
                ref={(el) => {
                  links.current[i] = el;
                }}
                href={c.game.href}
                aria-label={c.game.label}
                aria-describedby={descId(i)}
                tabIndex={i === current ? 0 : -1}
                onFocus={() => setActive(i)}
                onKeyDown={(e) => onKeyDown(e, i)}
                data-x={x}
                data-w={colW}
                className="group outline-none"
              >
                {/* Hover moves one step down the tonal ladder; focus gets the ink ring. */}
                <rect
                  x={x}
                  y={0}
                  width={colW}
                  height={height}
                  rx={2}
                  className="fill-transparent group-hover:fill-well"
                />
                {kinds.map(({ kind, slot }) => (
                  <Cell
                    key={slot}
                    kind={kind}
                    x={cx}
                    y={baseline - (slot + 1) * cellH - slot * CELL_GAP}
                    w={cellW}
                    h={cellH}
                    prefix={prefix}
                  />
                ))}
                {kinds.length === 0 && (
                  <rect
                    data-cell="none"
                    x={cx}
                    y={baseline - 1}
                    width={cellW}
                    height={1}
                    className="fill-rule"
                  />
                )}
                <rect
                  x={x + 1}
                  y={1}
                  width={Math.max(0, colW - 2)}
                  height={height - 2}
                  rx={2}
                  fill="none"
                  strokeWidth={2}
                  className="stroke-ink opacity-0 group-focus-visible:opacity-100"
                />
              </a>
            );
          })}
        </svg>
      </figure>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-sm text-ink-soft">
        {(
          [
            ["blunder", "Blunder"],
            ["mistake", "Mistake"],
            ["inaccuracy", "Inaccuracy"],
          ] as const
        ).map(([kind, text]) => (
          <span key={kind} className="flex items-center gap-2">
            <Swatch kind={kind} prefix={prefix} />
            <span>{text}</span>
          </span>
        ))}
        <span className="text-ink-faint">Each block is one move.</span>
      </div>
    </div>
  );
}
