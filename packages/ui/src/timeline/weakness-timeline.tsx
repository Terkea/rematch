"use client";

import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { TimelineSeries, WeaknessTimelineProps } from "../fixtures/types.js";
import { CaretLeftIcon, CaretRightIcon, Icon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
import { touchTarget } from "../primitives/touch.js";
import { type StateStyle, stateStyle, type WeaknessState, weaknessStates } from "./states.js";

export type { TimelineSeries, WeaknessTimelineProps } from "../fixtures/types.js";

type Segment = TimelineSeries["segments"][number];
type TimelineEvent = NonNullable<WeaknessTimelineProps["events"]>[number];

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY = 86_400_000;

/** Days since the epoch of an ISO date ("2026-09-01"), in UTC so no time zone shifts a day. */
function day(iso: string): number {
  const [y = 0, m = 1, d = 1] = iso.slice(0, 10).split("-").map(Number);
  return Date.UTC(y, m - 1, d) / DAY;
}

/** "10 Apr 2026" (British order, no locale lookups). */
export function formatDate(iso: string): string {
  const date = new Date(day(iso) * DAY);
  return `${date.getUTCDate()} ${MONTHS[date.getUTCMonth()]} ${date.getUTCFullYear()}`;
}

/**
 * A calendar position: months since year 0, plus the day's share of its own month. The axis is a
 * calendar, so every month is the same width (reports and plans run by the month) and the month
 * ruling is regular; days spread evenly inside their month.
 */
function monthPos(iso: string): number {
  const [y = 0, m = 1, d = 1] = iso.slice(0, 10).split("-").map(Number);
  const days = new Date(Date.UTC(y, m, 0)).getUTCDate();
  return y * 12 + (m - 1) + (d - 1) / days;
}

/** Where a date sits across the range on the calendar axis, 0 to 1, clamped. */
export function rangeFraction(iso: string, range: WeaknessTimelineProps["range"]): number {
  const from = monthPos(range.from);
  const span = monthPos(range.to) - from;
  if (span <= 0) return 0;
  return Math.min(1, Math.max(0, (monthPos(iso) - from) / span));
}

const pc = (f: number) => `${(f * 100).toFixed(3)}%`;

/** The text alternative of one lane: every state in order, then the onset. */
export function seriesText(s: TimelineSeries): string {
  const sorted = [...s.segments].sort((x, y) => day(x.from) - day(y.from));
  const parts = sorted.map((seg, i) => {
    const label = stateStyle[seg.state].label.toLowerCase();
    const last = i === sorted.length - 1;
    const end = last ? (seg.to === null ? " to now" : ` until ${formatDate(seg.to)}`) : "";
    return `${label} from ${formatDate(seg.from)}${end}`;
  });
  const onset = s.onset
    ? ` Estimated onset ${formatDate(s.onset.date)}, between ${formatDate(s.onset.low)} and ${formatDate(s.onset.high)}.`
    : "";
  return `${s.label}: ${parts.join("; ")}.${onset}`;
}

// Lane geometry in px (the x axis is in % of the lane, so nothing needs measuring).
const LANE_H = 40;
const TRACK = { y: 9, h: 26 };
const BAR: Record<StateStyle["weight"], { y: number; h: number }> = {
  full: { y: 12, h: 20 },
  band: { y: 18, h: 8 },
  hairline: { y: 21, h: 2 },
};
const WHISKER_Y = 4;
const EVENTS_H = 28;

const patternId = (prefix: string, state: WeaknessState) => `${prefix}-${state}`;

/** Paint for a state: its token, or its SVG pattern drawn in that token. */
const paint = (prefix: string, state: WeaknessState) =>
  stateStyle[state].pattern === "none"
    ? stateStyle[state].fill
    : `url(#${patternId(prefix, state)})`;

/**
 * The state patterns, built from tokens. Each patterned state also gets a solid underpaint of the
 * lane's well (from the track), so the dots and hatching sit on a known surface for contrast:
 * `pen` on `well` 4.14:1 day and 6.11:1 night, `good` on `well` 5.56:1 and 10.20:1.
 */
function PatternDefs({ prefix }: { prefix: string }) {
  return (
    <svg aria-hidden="true" width={0} height={0} className="absolute" focusable="false">
      <defs>
        {weaknessStates.map((state) => {
          const s = stateStyle[state];
          if (s.pattern === "dots") {
            return (
              <pattern
                key={state}
                id={patternId(prefix, state)}
                width={5}
                height={5}
                patternUnits="userSpaceOnUse"
              >
                <circle cx={2.5} cy={2.5} r={1.35} style={{ fill: s.fill }} />
              </pattern>
            );
          }
          if (s.pattern === "hatch") {
            return (
              <pattern
                key={state}
                id={patternId(prefix, state)}
                width={6}
                height={6}
                patternUnits="userSpaceOnUse"
                patternTransform="rotate(45)"
              >
                <rect x={0} y={0} width={2.25} height={6} style={{ fill: s.fill }} />
                {s.angle === "cross" && (
                  <rect x={0} y={0} width={6} height={2.25} style={{ fill: s.fill }} />
                )}
              </pattern>
            );
          }
          return null;
        })}
      </defs>
    </svg>
  );
}

/** A legend swatch: the state's own mark at small size, on the same well. */
function Swatch({ prefix, state }: { prefix: string; state: WeaknessState }) {
  const bar = BAR[stateStyle[state].weight];
  const scale = 14 / BAR.full.h;
  return (
    <svg aria-hidden="true" width={28} height={18} className="shrink-0">
      <rect x={0} y={0} width={28} height={18} rx={2} className="fill-well" />
      <rect
        x={3}
        y={2 + (bar.y - BAR.full.y) * scale}
        width={22}
        height={Math.max(2, bar.h * scale)}
        style={{ fill: paint(prefix, state) }}
      />
    </svg>
  );
}

function OnsetSwatch() {
  return (
    <svg aria-hidden="true" width={28} height={18} className="shrink-0">
      <line x1={6} x2={22} y1={4} y2={4} strokeWidth={1.5} className="stroke-pen" />
      <line x1={6} x2={6} y1={1.5} y2={6.5} strokeWidth={1.5} className="stroke-pen" />
      <line x1={22} x2={22} y1={1.5} y2={6.5} strokeWidth={1.5} className="stroke-pen" />
      <line
        x1={14}
        x2={14}
        y1={2}
        y2={17}
        strokeWidth={2.5}
        strokeLinecap="round"
        className="stroke-pen"
      />
    </svg>
  );
}

const PLAN_PATH = "M0 7 L6 13 L0 19 L-6 13 Z";

function EventMark({ kind }: { kind: TimelineEvent["kind"] }) {
  return kind === "plan" ? (
    <path d={PLAN_PATH} strokeWidth={1.5} className="fill-sheet stroke-pencil" />
  ) : (
    <rect x={-4.5} y={8.5} width={9} height={9} rx={1} className="fill-pencil" />
  );
}

function EventSwatch({ kind }: { kind: TimelineEvent["kind"] }) {
  return (
    <svg aria-hidden="true" width={28} height={18} className="shrink-0" overflow="visible">
      <g transform="translate(14 -4)">
        <EventMark kind={kind} />
      </g>
    </svg>
  );
}

function Key({ prefix, events }: { prefix: string; events: TimelineEvent[] }) {
  const kinds = (["plan", "drill"] as const).filter((k) => events.some((e) => e.kind === k));
  return (
    <ul
      aria-label="Key"
      className="m-0 flex list-none flex-wrap gap-x-5 gap-y-2 p-0 text-sm text-ink-soft"
    >
      {weaknessStates.map((state) => (
        <li key={state} className="flex items-center gap-2">
          <Swatch prefix={prefix} state={state} />
          {stateStyle[state].label}
        </li>
      ))}
      <li className="flex items-center gap-2">
        <OnsetSwatch />
        Onset, with its likely range
      </li>
      {kinds.map((k) => (
        <li key={k} className="flex items-center gap-2">
          <EventSwatch kind={k} />
          {k === "plan" ? "Plan started" : "Drill set"}
        </li>
      ))}
    </ul>
  );
}

interface Month {
  iso: string;
  f: number;
  label: string;
}

/** First-of-month ticks inside the range, thinned to at most about 12 labels. */
function monthTicks(range: WeaknessTimelineProps["range"]): Month[] {
  const out: Month[] = [];
  const [y0 = 0, m0 = 1] = range.from.split("-").map(Number);
  const end = day(range.to);
  let y = y0;
  let m = m0 - 1;
  if (day(range.from) > Date.UTC(y, m, 1) / DAY) m += 1;
  for (;;) {
    if (m > 11) {
      y += 1;
      m = 0;
    }
    const iso = `${y}-${String(m + 1).padStart(2, "0")}-01`;
    if (day(iso) > end) break;
    out.push({ iso, f: rangeFraction(iso, range), label: MONTHS[m] ?? "" });
    m += 1;
  }
  const step = Math.ceil(out.length / 12);
  return out
    .filter((_, i) => i % step === 0)
    .map((t, i) => ({
      ...t,
      label: i === 0 || t.label === "Jan" ? `${t.label} ${t.iso.slice(0, 4)}` : t.label,
    }));
}

/** A lane's segments in date order, each with its span across the range. */
interface Span {
  seg: Segment;
  a: number;
  b: number;
}

function spans(series: TimelineSeries, range: WeaknessTimelineProps["range"]): Span[] {
  return [...series.segments]
    .sort((x, y) => day(x.from) - day(y.from))
    .map((seg) => ({
      seg,
      a: rangeFraction(seg.from, range),
      b: seg.to === null ? 1 : rangeFraction(seg.to, range),
    }));
}

/** The state at `range.to`: the segment covering the range's end, else the latest one. */
export function stateAtEnd(
  series: TimelineSeries,
  range: WeaknessTimelineProps["range"],
): WeaknessState | undefined {
  const end = day(range.to);
  const sorted = [...series.segments].sort((x, y) => day(x.from) - day(y.from));
  const covering = sorted.filter((s) => day(s.from) <= end && (s.to === null || day(s.to) >= end));
  return (covering[covering.length - 1] ?? sorted[sorted.length - 1])?.state;
}

/** "Resolved, 20 Aug 2026 to 5 Oct 2026". */
const spanText = (seg: Segment) =>
  `${stateStyle[seg.state].label}, ${formatDate(seg.from)} to ${seg.to === null ? "now" : formatDate(seg.to)}`;

interface Cell {
  lane: number;
  index: number;
}

const sameCell = (x: Cell | null, y: Cell | null) =>
  x !== null && y !== null && x.lane === y.lane && x.index === y.index;

function Lane({
  series,
  laneIndex,
  segs,
  prefix,
  months,
  events,
  range,
  cursor,
  shown,
  onHover,
  onFocusCell,
  onKey,
  cellRef,
}: {
  series: TimelineSeries;
  laneIndex: number;
  segs: Span[];
  prefix: string;
  months: Month[];
  events: { f: number; key: string }[];
  range: WeaknessTimelineProps["range"];
  cursor: Cell | null;
  shown: Cell | null;
  onHover: (cell: Cell | null) => void;
  onFocusCell: (cell: Cell | null) => void;
  onKey: (e: KeyboardEvent<HTMLDivElement>, cell: Cell) => void;
  cellRef: (cell: Cell, el: HTMLDivElement | null) => void;
}) {
  return (
    <div data-lane={series.id} className="relative" style={{ height: LANE_H }}>
      <svg aria-hidden="true" width="100%" height={LANE_H} className="block overflow-visible">
        <rect x="0" y={TRACK.y} width="100%" height={TRACK.h} rx={2} className="fill-well" />
        {/* Month ruling: a sheet hairline pressed into the well at each month start. */}
        {months.map((m) => (
          <line
            key={m.iso}
            x1={pc(m.f)}
            x2={pc(m.f)}
            y1={TRACK.y}
            y2={TRACK.y + TRACK.h}
            strokeWidth={1}
            className="stroke-sheet"
          />
        ))}
        {events.map((ev) => (
          <line
            key={ev.key}
            data-event-line
            x1={pc(ev.f)}
            x2={pc(ev.f)}
            y1={0}
            y2={LANE_H}
            strokeWidth={1}
            strokeDasharray="2 3"
            className="stroke-pencil"
          />
        ))}
        {segs.map(({ seg, a, b }) => {
          const s = stateStyle[seg.state];
          const bar = BAR[s.weight];
          return (
            <rect
              key={`${seg.state}-${seg.from}`}
              data-segment={seg.state}
              x={pc(a)}
              y={bar.y}
              width={pc(b - a)}
              height={bar.h}
              // A 2px well gap between neighbouring bars (a hairline has nothing to separate).
              strokeWidth={s.weight === "hairline" ? 0 : 2}
              style={{ fill: paint(prefix, seg.state), stroke: "var(--color-well)" }}
            />
          );
        })}
        {series.onset && <Onset onset={series.onset} range={range} />}
        {/* The reveal: a sheet cover slides off to the right on the one clock (`--wipe`). */}
        <rect
          data-wipe
          x="0"
          y="0"
          width="100%"
          height={LANE_H}
          className="pointer-events-none fill-sheet"
          style={{
            transformBox: "fill-box",
            transform: "translateX(calc(var(--wipe, 1) * 100%))",
          }}
        />
      </svg>
      {segs.length === 0 && (
        <span
          className="absolute left-3 text-xs text-ink-faint"
          style={{ top: TRACK.y, lineHeight: `${TRACK.h}px` }}
        >
          No data yet
        </span>
      )}
      {/* The keyboard and pointer layer: one cell over each segment, a roving tab stop. */}
      {segs.map(({ seg, a, b }, index) => {
        const cell = { lane: laneIndex, index };
        return (
          <div
            key={`${seg.state}-${seg.from}`}
            ref={(el) => cellRef(cell, el)}
            role="img"
            data-cell={seg.state}
            aria-label={`${series.label}, ${spanText(seg)}, state ${index + 1} of ${segs.length}`}
            tabIndex={sameCell(cursor, cell) ? 0 : -1}
            data-shown={sameCell(shown, cell) ? "" : undefined}
            className="absolute rounded-[2px] outline-ink focus-visible:outline-2 focus-visible:outline-offset-1 data-shown:outline-1"
            style={{ left: pc(a), width: pc(b - a), top: TRACK.y, height: TRACK.h }}
            onPointerEnter={() => onHover(cell)}
            onPointerDown={() => onHover(cell)}
            onPointerLeave={() => onHover(null)}
            onFocus={() => onFocusCell(cell)}
            onBlur={() => onFocusCell(null)}
            onKeyDown={(e) => onKey(e, cell)}
          />
        );
      })}
    </div>
  );
}

function Onset({
  onset,
  range,
}: {
  onset: NonNullable<TimelineSeries["onset"]>;
  range: WeaknessTimelineProps["range"];
}) {
  const lo = pc(rangeFraction(onset.low, range));
  const hi = pc(rangeFraction(onset.high, range));
  const at = pc(rangeFraction(onset.date, range));
  return (
    <g data-onset>
      <line
        data-onset-whisker
        x1={lo}
        x2={hi}
        y1={WHISKER_Y}
        y2={WHISKER_Y}
        strokeWidth={1.5}
        className="stroke-pen"
      />
      <line x1={lo} x2={lo} y1={1.5} y2={6.5} strokeWidth={1.5} className="stroke-pen" />
      <line x1={hi} x2={hi} y1={1.5} y2={6.5} strokeWidth={1.5} className="stroke-pen" />
      {/* The tick on a sheet halo, so it reads over any state's paint. */}
      <line
        x1={at}
        x2={at}
        y1={2}
        y2={LANE_H - 2}
        strokeWidth={5.5}
        strokeLinecap="round"
        className="stroke-sheet"
      />
      <line
        data-onset-tick
        x1={at}
        x2={at}
        y1={2}
        y2={LANE_H - 2}
        strokeWidth={2.5}
        strokeLinecap="round"
        className="stroke-pen"
      />
    </g>
  );
}

/** Greedy rows for event markers: one closer than `gap` to the last marker in a row moves down. */
export function eventRows(fs: number[], gap = 0.04): number[] {
  const last: number[] = [];
  return fs.map((f) => {
    let row = last.findIndex((l) => f - l >= gap);
    if (row < 0) row = last.length < 2 ? last.length : 1;
    last[row] = f;
    return row;
  });
}

const EVENT_ROW = 13;

function EventsLane({
  events,
}: {
  events: { f: number; key: string; kind: TimelineEvent["kind"]; row: number }[];
}) {
  const h = EVENTS_H + EVENT_ROW * Math.max(0, ...events.map((e) => e.row));
  return (
    <svg aria-hidden="true" width="100%" height={h} className="block overflow-visible" data-events>
      {events.map((ev, i) => (
        <svg
          key={ev.key}
          aria-hidden="true"
          data-event={ev.kind}
          data-row={ev.row}
          x={pc(ev.f)}
          y={ev.row * EVENT_ROW}
          overflow="visible"
        >
          <line
            x1={0}
            x2={0}
            y1={19}
            y2={h - ev.row * EVENT_ROW}
            strokeWidth={1}
            strokeDasharray="2 3"
            className="stroke-pencil"
          />
          <EventMark kind={ev.kind} />
          <text
            x={9}
            y={11}
            className="fill-ink-soft font-sans text-xs font-medium"
            style={{ fontVariantNumeric: "lining-nums tabular-nums" }}
          >
            {i + 1}
          </text>
        </svg>
      ))}
    </svg>
  );
}

/** "1 Nov": the range's end, labelled at the axis's right edge. */
const shortDate = (iso: string) => formatDate(iso).replace(/ \d{4}$/, "");
/** Room a month label needs (px) before it would run into the end label. */
const END_LABEL_ROOM = 84;

const LABEL_CELL = "sticky left-0 z-10 flex flex-col justify-center bg-sheet pr-3";
const NAV_BUTTON = cn(
  "relative flex size-8 items-center justify-center rounded-field text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
  touchTarget,
);

interface Overflow {
  overflows: boolean;
  earlier: boolean;
  later: boolean;
  plot: number;
}

/**
 * The weakness timeline (spec §12b): one swimlane per weakness on a shared time axis. Each lane is
 * painted by state, and each state differs by pattern and by bar shape as well as by colour. The
 * onset is a pen tick with a whisker across its likely range, and plan and drill events run as
 * dashed pencil rules through every lane.
 *
 * It is one composite widget: a roving tab stop over the segments, where ←/→ move along a lane,
 * ↑/↓ between lanes and Home/End to a lane's ends. The focused or hovered segment's state and
 * dates show in one fixed line above the chart, so nothing is ever clipped or covers a label.
 * Below a 640px container the lanes scroll inside their own region with the labels pinned, opening
 * on the latest months. Pass `drawIn` to reveal it left to right once.
 */
export function WeaknessTimeline({
  series,
  events = [],
  range,
  drawIn = false,
  label = "Weakness timeline",
  className,
}: WeaknessTimelineProps & { drawIn?: boolean; label?: string; className?: string }) {
  const rawId = useId();
  const prefix = `wt${rawId.replace(/[^a-zA-Z0-9]/g, "")}`;
  const months = monthTicks(range);
  const lanes = useMemo(() => series.map((s) => spans(s, range)), [series, range]);
  const placed = useMemo(() => {
    const sorted = [...events].sort((x, y) => day(x.date) - day(y.date));
    const fs = sorted.map((ev) => rangeFraction(ev.date, range));
    const rows = eventRows(fs);
    return sorted.map((ev, i) => ({
      ...ev,
      f: fs[i] ?? 0,
      row: rows[i] ?? 0,
      key: `${i}-${ev.kind}-${ev.date}`,
    }));
  }, [events, range]);

  // The composite widget: one roving cursor, plus what the pointer is over.
  const firstLane = lanes.findIndex((l) => l.length > 0);
  const [cursor, setCursor] = useState<Cell | null>(null);
  const roving: Cell | null =
    cursor ??
    (firstLane < 0 ? null : { lane: firstLane, index: (lanes[firstLane]?.length ?? 1) - 1 });
  const [hover, setHover] = useState<Cell | null>(null);
  const [focused, setFocused] = useState<Cell | null>(null);
  const shown = focused ?? hover;
  const cells = useRef(new Map<string, HTMLDivElement>());
  const cellRef = useCallback((c: Cell, el: HTMLDivElement | null) => {
    const k = `${c.lane}:${c.index}`;
    if (el) cells.current.set(k, el);
    else cells.current.delete(k);
  }, []);
  const moveTo = (c: Cell) => {
    setCursor(c);
    cells.current.get(`${c.lane}:${c.index}`)?.focus();
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>, c: Cell) => {
    const lane = lanes[c.lane] ?? [];
    const here = lane[c.index];
    let next: Cell | null = null;
    if (e.key === "ArrowLeft") next = { ...c, index: Math.max(0, c.index - 1) };
    else if (e.key === "ArrowRight") next = { ...c, index: Math.min(lane.length - 1, c.index + 1) };
    else if (e.key === "Home") next = { ...c, index: 0 };
    else if (e.key === "End") next = { ...c, index: lane.length - 1 };
    else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      const step = e.key === "ArrowUp" ? -1 : 1;
      let l = c.lane + step;
      while (l >= 0 && l < lanes.length && (lanes[l]?.length ?? 0) === 0) l += step;
      const target = lanes[l];
      if (target && here) {
        // Keep the place in time: the segment under the current one's midpoint.
        const mid = (here.a + here.b) / 2;
        const i = target.findIndex((s) => mid >= s.a && mid <= s.b);
        next = { lane: l, index: i < 0 ? target.length - 1 : i };
      } else next = c;
    } else return;
    e.preventDefault();
    if (next) moveTo(next);
  };
  const shownSpan = shown ? lanes[shown.lane]?.[shown.index] : undefined;
  const shownSeries = shown ? series[shown.lane] : undefined;

  // The one clock: `--wipe` runs 0 to 1 over `dur.reveal` on `ease.pen`, and is 1 at rest and
  // under reduced motion (M10). Any input finishes it over `dur.quick` (M7).
  const reduced = useReducedMotion();
  const revealing = drawIn && !reduced;
  const wipe = useMotionValue(revealing ? 0 : 1);
  useEffect(() => {
    if (!revealing) {
      wipe.jump(1);
      return;
    }
    wipe.jump(0);
    const controls = animate(wipe, 1, { duration: dur.reveal, ease: ease.pen });
    return () => controls.stop();
  }, [revealing, wipe]);
  const finish = () => {
    if (wipe.get() < 1) animate(wipe, 1, { duration: dur.quick, ease: ease.out });
  };

  // Overflow: the scroller is a named, focusable region only while it actually scrolls.
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const axisRef = useRef<HTMLDivElement | null>(null);
  const [overflow, setOverflow] = useState<Overflow>({
    overflows: false,
    earlier: false,
    later: false,
    plot: 0,
  });
  const measure = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const overflows = el.scrollWidth > el.clientWidth + 1;
    const next = {
      overflows,
      earlier: overflows && el.scrollLeft > 1,
      later: overflows && el.scrollLeft + el.clientWidth < el.scrollWidth - 1,
      plot: axisRef.current?.clientWidth ?? 0,
    };
    setOverflow((o) =>
      o.overflows === next.overflows &&
      o.earlier === next.earlier &&
      o.later === next.later &&
      o.plot === next.plot
        ? o
        : next,
    );
  }, []);
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // Open on the latest months: where the player is now.
    if (el.scrollWidth > el.clientWidth) el.scrollLeft = el.scrollWidth;
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [measure]);
  const page = (dir: -1 | 1) => {
    const el = scrollRef.current;
    if (el)
      el.scrollBy({ left: dir * el.clientWidth * 0.6, behavior: reduced ? "auto" : "smooth" });
  };
  const monthRoom = (m: Month) =>
    overflow.plot > 0 ? m.f * overflow.plot < overflow.plot - END_LABEL_ROOM : m.f < 0.85;

  return (
    <figure
      aria-label={label}
      className={cn("@container relative m-0 flex flex-col gap-4", className)}
    >
      <PatternDefs prefix={prefix} />
      <Key prefix={prefix} events={placed} />
      {/* The readout: one fixed line above the chart, so it is never clipped and never covers a label. */}
      <p
        data-readout
        aria-hidden="true"
        className="m-0 flex h-10 min-w-0 items-start gap-2 text-sm @min-[640px]:h-6"
      >
        {shownSpan && shownSeries ? (
          <>
            <span className="mt-px shrink-0">
              <Swatch prefix={prefix} state={shownSpan.seg.state} />
            </span>
            <span className="line-clamp-2">
              <span className="font-medium text-ink">{shownSeries.label}:</span>{" "}
              <span className="text-ink-soft">{spanText(shownSpan.seg)}</span>
            </span>
          </>
        ) : (
          <span className="text-ink-faint">
            Hover, tap or use the arrow keys to read a state's dates.
          </span>
        )}
      </p>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: these handlers only finish the reveal; the cells inside are the widget. */}
      <div
        ref={scrollRef}
        data-timeline-scroll
        {...(overflow.overflows
          ? { role: "region", "aria-label": `${label}, scrolls sideways`, tabIndex: 0 }
          : {})}
        style={{ overflowX: "auto" }}
        className="rounded-field pb-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        onPointerDown={finish}
        onKeyDown={finish}
        onWheel={finish}
        onScroll={measure}
      >
        <motion.div
          role="group"
          aria-label={`${label}: ${series.length} weaknesses. Arrow keys move between states and lanes.`}
          className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-center @max-[640px]:min-w-[38rem] @min-[640px]:grid-cols-[13rem_minmax(0,1fr)]"
          style={{ "--wipe": wipe } as never}
        >
          {placed.length > 0 && (
            <>
              <div className={cn(LABEL_CELL, "self-stretch text-sm text-ink-soft")}>
                Plan and drills
              </div>
              <EventsLane events={placed} />
            </>
          )}
          {series.map((s, laneIndex) => {
            const now = stateAtEnd(s, range);
            return (
              <div key={s.id} className="contents">
                <div className={cn(LABEL_CELL, "py-1")} style={{ minHeight: LANE_H }}>
                  <span className="text-sm leading-tight font-medium text-ink">{s.label}</span>
                  {now && (
                    <span
                      data-now
                      className="mt-0.5 flex items-center gap-1.5 text-xs text-ink-soft"
                    >
                      <MiniSwatch prefix={prefix} state={now} />
                      Now {stateStyle[now].label.toLowerCase()}
                    </span>
                  )}
                </div>
                <Lane
                  series={s}
                  laneIndex={laneIndex}
                  segs={lanes[laneIndex] ?? []}
                  prefix={prefix}
                  months={months}
                  events={placed}
                  range={range}
                  cursor={roving}
                  shown={shown}
                  onHover={setHover}
                  onFocusCell={(c) => {
                    setFocused(c);
                    if (c) {
                      setCursor(c);
                      finish();
                    }
                  }}
                  onKey={onKey}
                  cellRef={cellRef}
                />
              </div>
            );
          })}
          <div className={cn(LABEL_CELL, "flex-row items-center gap-1 self-stretch")}>
            {overflow.earlier && (
              <button
                type="button"
                aria-label="Show earlier months"
                className={NAV_BUTTON}
                onClick={() => page(-1)}
              >
                <Icon icon={CaretLeftIcon} size={16} />
              </button>
            )}
            {overflow.later && (
              <button
                type="button"
                aria-label="Show later months"
                className={NAV_BUTTON}
                onClick={() => page(1)}
              >
                <Icon icon={CaretRightIcon} size={16} />
              </button>
            )}
            {overflow.earlier && <span className="text-xs text-ink-faint">Earlier</span>}
          </div>
          <div ref={axisRef} aria-hidden="true" data-axis className="relative h-6 overflow-hidden">
            <div className="absolute inset-x-0 top-0 h-px bg-rule-strong" />
            {months.map((m) => (
              <div key={m.iso} className="absolute top-0 flex flex-col" style={{ left: pc(m.f) }}>
                <span className="h-1.5 w-px bg-rule-strong" />
                {m.f < 1 && monthRoom(m) && (
                  <span className="pl-1 text-xs whitespace-nowrap text-ink-faint">{m.label}</span>
                )}
              </div>
            ))}
            <div className="absolute top-0 right-0 flex flex-col items-end">
              <span className="h-1.5 w-px bg-rule-strong" />
              <span data-range-end className="pr-0.5 text-xs whitespace-nowrap text-ink-soft">
                {shortDate(range.to)}
              </span>
            </div>
          </div>
        </motion.div>
      </div>
      {placed.length > 0 && (
        <ol
          aria-label="Plan and drills"
          className="m-0 flex list-none flex-col gap-1 p-0 text-sm text-ink-soft"
        >
          {placed.map((ev, i) => (
            <li key={ev.key} className="flex items-center gap-2">
              <span
                className="w-4 text-right font-medium text-ink"
                style={{ fontVariantNumeric: "lining-nums tabular-nums" }}
              >
                {i + 1}
              </span>
              <EventSwatch kind={ev.kind} />
              <span>
                {formatDate(ev.date)}: {ev.label}
                <span className="sr-only"> ({ev.kind === "plan" ? "plan" : "drill"})</span>
              </span>
            </li>
          ))}
        </ol>
      )}
      <div className="sr-only">
        {series.map((s) => (
          <p key={s.id}>{s.segments.length ? seriesText(s) : `${s.label}: no data yet.`}</p>
        ))}
      </div>
    </figure>
  );
}

/** A 12 × 8 swatch for the "Now" line. */
function MiniSwatch({ prefix, state }: { prefix: string; state: WeaknessState }) {
  const bar = BAR[stateStyle[state].weight];
  const scale = 6 / BAR.full.h;
  return (
    <svg aria-hidden="true" width={14} height={10} className="shrink-0">
      <rect x={0} y={0} width={14} height={10} rx={1.5} className="fill-well" />
      <rect
        x={1.5}
        y={2 + (bar.y - BAR.full.y) * scale}
        width={11}
        height={Math.max(1.5, bar.h * scale)}
        style={{ fill: paint(prefix, state) }}
      />
    </svg>
  );
}
