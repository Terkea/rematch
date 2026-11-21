"use client";

import { type CSSProperties, type PointerEvent, useEffect, useId, useRef, useState } from "react";
import type { TimelineSeries, WeaknessTimelineProps } from "../fixtures/types.js";
import { cn } from "../lib/index.js";
import { animate, dur, ease, motion, useMotionValue, useReducedMotion } from "../motion/index.js";
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
  const parts = s.segments.map((seg, i) => {
    const label = stateStyle[seg.state].label.toLowerCase();
    const last = i === s.segments.length - 1;
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

interface Hover {
  lane: string;
  index: number;
}

function Lane({
  series,
  range,
  prefix,
  months,
  events,
  hover,
  onHover,
}: {
  series: TimelineSeries;
  range: WeaknessTimelineProps["range"];
  prefix: string;
  months: Month[];
  events: TimelineEvent[];
  hover: number | null;
  onHover: (index: number | null) => void;
}) {
  const segs = series.segments.map((seg: Segment) => {
    const a = rangeFraction(seg.from, range);
    const b = seg.to === null ? 1 : rangeFraction(seg.to, range);
    return { seg, a, b };
  });
  const onPointer = (e: PointerEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    if (box.width <= 0) return;
    const f = (e.clientX - box.left) / box.width;
    const i = segs.findIndex(({ a, b }) => f >= a && f <= b);
    onHover(i < 0 ? null : i);
  };
  const shown = hover === null ? undefined : segs[hover];

  return (
    <div data-lane={series.id} className="relative" style={{ height: LANE_H }}>
      <svg
        aria-hidden="true"
        width="100%"
        height={LANE_H}
        className="block overflow-visible"
        onPointerMove={onPointer}
        onPointerDown={onPointer}
        onPointerLeave={() => onHover(null)}
      >
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
            key={`${ev.kind}-${ev.date}`}
            data-event-line={ev.kind}
            x1={pc(rangeFraction(ev.date, range))}
            x2={pc(rangeFraction(ev.date, range))}
            y1={0}
            y2={LANE_H}
            strokeWidth={1}
            strokeDasharray="2 3"
            className="stroke-pencil"
          />
        ))}
        {segs.map(({ seg, a, b }, i) => {
          const s = stateStyle[seg.state];
          const bar = BAR[s.weight];
          return (
            <rect
              key={`${seg.state}-${seg.from}`}
              data-segment={seg.state}
              data-hover={hover === i ? "" : undefined}
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
        {shown && (
          <rect
            x={pc(shown.a)}
            y={TRACK.y}
            width={pc(shown.b - shown.a)}
            height={TRACK.h}
            rx={2}
            fill="none"
            strokeWidth={1.5}
            className="stroke-ink"
          />
        )}
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
      {shown && <Readout seg={shown.seg} a={shown.a} b={shown.b} />}
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

/** The hover readout: the state and its dates, floating under the lane (it is lifted, so it may shadow). */
function Readout({ seg, a, b }: { seg: Segment; a: number; b: number }) {
  const mid = (a + b) / 2;
  const style: CSSProperties =
    mid < 0.2
      ? { left: pc(Math.max(0, a)) }
      : mid > 0.8
        ? { right: pc(1 - b) }
        : { left: pc(mid), transform: "translateX(-50%)" };
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute top-full z-20 -mt-1 rounded-field bg-ink px-2.5 py-1 text-sm whitespace-nowrap text-on-ink shadow-lift"
      style={style}
    >
      <span className="font-medium">{stateStyle[seg.state].label}</span> {formatDate(seg.from)} to{" "}
      {seg.to === null ? "now" : formatDate(seg.to)}
    </div>
  );
}

function EventsLane({
  events,
  range,
}: {
  events: TimelineEvent[];
  range: WeaknessTimelineProps["range"];
}) {
  return (
    <svg
      aria-hidden="true"
      width="100%"
      height={EVENTS_H}
      className="block overflow-visible"
      data-events
    >
      {events.map((ev, i) => (
        <svg
          key={`${ev.kind}-${ev.date}`}
          aria-hidden="true"
          data-event={ev.kind}
          x={pc(rangeFraction(ev.date, range))}
          y={0}
          overflow="visible"
        >
          <line
            x1={0}
            x2={0}
            y1={19}
            y2={EVENTS_H}
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

const LABEL_CELL = "sticky left-0 z-10 flex flex-col justify-center bg-sheet pr-3";

/**
 * The weakness timeline (spec §12b): one swimlane per weakness on a shared time axis. Each lane is
 * painted by state, and each state differs by pattern and by bar shape as well as by colour. The
 * onset is a pen tick with a whisker across its likely range, and plan and drill events run as
 * dashed pencil rules through every lane. Below a 640px container it scrolls sideways inside its
 * own region, with the lane labels pinned. Pass `drawIn` to reveal it left to right once.
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
  const [hover, setHover] = useState<Hover | null>(null);
  const months = monthTicks(range);
  const sorted = [...events].sort((x, y) => day(x.date) - day(y.date));

  // The one clock: `--wipe` runs 0 to 1 over `dur.reveal` on `ease.pen`, and is 1 at rest and
  // under reduced motion (M10). Input finishes it over `dur.quick` (M7).
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
  // On a narrow screen the region opens on its latest months: where the player is now.
  const scrollRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const el = scrollRef.current;
    if (el && el.scrollWidth > el.clientWidth) el.scrollLeft = el.scrollWidth;
  }, []);
  const finish = () => {
    if (wipe.get() < 1) animate(wipe, 1, { duration: dur.quick, ease: ease.out });
  };

  return (
    <figure className={cn("@container relative m-0 flex flex-col gap-5", className)}>
      <PatternDefs prefix={prefix} />
      <Key prefix={prefix} events={sorted} />
      <section
        ref={scrollRef}
        data-timeline-scroll
        aria-label={`${label}, scrolls sideways`}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling region must be keyboard-reachable.
        tabIndex={0}
        style={{ overflowX: "auto" }}
        className="rounded-field pb-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        onPointerDown={finish}
      >
        <motion.div
          className="grid grid-cols-[7.5rem_minmax(0,1fr)] @max-[640px]:min-w-[38rem] @min-[640px]:grid-cols-[13rem_minmax(0,1fr)]"
          style={{ "--wipe": wipe } as never}
        >
          {sorted.length > 0 && (
            <>
              <div className={cn(LABEL_CELL, "text-sm text-ink-soft")} style={{ height: EVENTS_H }}>
                Plan and drills
              </div>
              <EventsLane events={sorted} range={range} />
            </>
          )}
          {series.map((s) => {
            const now = s.segments[s.segments.length - 1]?.state;
            return (
              <div key={s.id} className="contents">
                <div className={LABEL_CELL} style={{ minHeight: LANE_H }}>
                  <span className="text-sm leading-tight font-medium text-ink">{s.label}</span>
                  {now && (
                    <span className="text-xs text-ink-faint">
                      Now {stateStyle[now].label.toLowerCase()}
                    </span>
                  )}
                </div>
                <Lane
                  series={s}
                  range={range}
                  prefix={prefix}
                  months={months}
                  events={sorted}
                  hover={hover?.lane === s.id ? hover.index : null}
                  onHover={(index) => setHover(index === null ? null : { lane: s.id, index })}
                />
              </div>
            );
          })}
          <div className={LABEL_CELL} />
          <div aria-hidden="true" className="relative h-6 overflow-hidden">
            <div className="absolute inset-x-0 top-0 h-px bg-rule-strong" />
            {months.map((m) => (
              <div key={m.iso} className="absolute top-0 flex flex-col" style={{ left: pc(m.f) }}>
                <span className="h-1.5 w-px bg-rule-strong" />
                {m.f < 0.95 && (
                  <span className="pl-1 text-xs whitespace-nowrap text-ink-faint">{m.label}</span>
                )}
              </div>
            ))}
          </div>
        </motion.div>
      </section>
      {sorted.length > 0 && (
        <ol
          aria-label="Plan and drills"
          className="m-0 flex list-none flex-col gap-1 p-0 text-sm text-ink-soft"
        >
          {sorted.map((ev, i) => (
            <li key={`${ev.kind}-${ev.date}`} className="flex items-center gap-2">
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
          <p key={s.id}>{seriesText(s)}</p>
        ))}
      </div>
    </figure>
  );
}
