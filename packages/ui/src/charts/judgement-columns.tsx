"use client";

import { Responsive } from "@visx/visx";
import { useId } from "react";
import { cn } from "../lib/index.js";
import { ChartPatterns, patternFill } from "./patterns.js";
import { CELL_GAP, COLUMNS_MIN_ROWS, cellHeight, columnsWidth } from "./scales.js";

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
  /** One entry per game, oldest first: the oldest is drawn on the left. */
  games: JudgementColumnsGame[];
  /** Accessible name of the chart. */
  label: string;
  className?: string;
}

type Kind = "blunder" | "mistake" | "inaccuracy";

/** The widest a column gets, so a handful of games stay unit columns rather than slabs. */
const COLUMN_MAX_W = 24;
const IGNORED_DIMENSIONS: ("height" | "top" | "left")[] = ["height", "top", "left"];

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const countText = (g: JudgementColumnsGame) =>
  g.blunders + g.mistakes + g.inaccuracies === 0
    ? "no marked moves"
    : [
        plural(g.blunders, "blunder", "blunders"),
        plural(g.mistakes, "mistake", "mistakes"),
        plural(g.inaccuracies, "inaccuracy", "inaccuracies"),
      ].join(", ");

/** Non-negative whole counts only: a bad count never draws a negative or fractional cell. */
const whole = (n: number) => (Number.isFinite(n) && n > 0 ? Math.floor(n) : 0);

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
  /** A legend swatch carries no `data-cell`, so only the chart's own cells count. */
  swatch?: boolean;
  kind: Kind;
  x: number;
  y: number;
  w: number;
  h: number;
  prefix: string;
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
 * number, so the chart needs no value axis. Red means "your mistake" here, as everywhere. Each
 * column links to its game's review. Below 480px the columns keep a 12px least width and the
 * chart scrolls inside its own container, never the page.
 */
export function JudgementColumns({ games, label, className }: JudgementColumnsProps) {
  const prefix = `jc${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const { parentRef, width: measured } = Responsive.useParentSize({
    debounceTime: 50,
    enableDebounceLeadingCall: true,
    ignoreDimensions: IGNORED_DIMENSIONS,
  });

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
  const svgW = columnsWidth(measured, games.length);
  const colW = games.length ? Math.min(COLUMN_MAX_W, svgW / games.length) : 0;
  const cellW = Math.max(2, colW - CELL_GAP);

  return (
    <div className={cn("relative flex flex-col gap-3", className)}>
      <ChartPatterns prefix={prefix} />
      <figure
        ref={parentRef}
        aria-label={label}
        data-part="scroller"
        className="m-0 w-full overflow-x-auto pb-1"
      >
        <svg width={svgW} height={height} className="block overflow-visible">
          <title>{label}</title>
          {counts.map((c, i) => {
            const x = i * colW;
            const cx = x + CELL_GAP / 2;
            // Each cell's slot is its height above the baseline, and its stable key.
            const kinds = [
              ...Array<Kind>(c.blunders).fill("blunder"),
              ...Array<Kind>(c.mistakes).fill("mistake"),
              ...Array<Kind>(c.inaccuracies).fill("inaccuracy"),
            ].map((kind, slot) => ({ kind, slot }));
            return (
              <a
                key={c.game.id}
                href={c.game.href}
                aria-label={c.game.label}
                data-x={x}
                className="group outline-none"
              >
                <title>{`${c.game.label}: ${countText(c.game)}`}</title>
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
