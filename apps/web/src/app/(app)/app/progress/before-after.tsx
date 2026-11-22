import { dayText, type ExplainEntry, ExplainPopover } from "@ci/ui/profile";
import { stateStyle } from "@ci/ui/timeline";
import type { WeaknessDetail } from "../../../../server/profile";
import { seriesGlossary } from "../../../../server/profile/glossary";

// Before and after (spec §10a.4): the rates either side of a detected change, straight from the
// weakness's own history rows (`weakness_state_history.rate_before` / `rate_after`). Rust writes a
// confirmed change as two rows of the same game: a transition (no onset fields) and an onset row
// whose onset is the change point, where the rates split. The words follow §12a: a change is
// "detected", never "fixed", and nothing names training as its cause.

const CALLED = ["improving", "resolved", "relapsed"] as const;
type Called = (typeof CALLED)[number];
const count = new Intl.NumberFormat("en-GB");

export type BeforeAfterRow = {
  seriesKey: string;
  label: string;
  /** The weakness's glossary entry, or null when the glossary has none. */
  explain: ExplainEntry | null;
  state: Called;
  before: number;
  after: number;
  /** The change point from the same-game onset row, with its likely range (ISO days). */
  onset: { day: string; low: string | null; high: string | null } | null;
  /** The game that confirmed the change (ISO day). */
  confirmed: string | null;
  /** The after side's counts (the row's stored evidence: the games since the onset). */
  afterCounts: { events: number; chances: number } | null;
};

type History = WeaknessDetail["history"][number];

const fin = (v: number | null): v is number => v !== null && Number.isFinite(v);

export function isoDay(d: Date): string {
  return d.toISOString().slice(0, 10);
}

const pct = (share: number) => `${(share * 100).toFixed(1)}%`;
const sameTime = (a: Date | null, b: Date | null) =>
  a !== null && b !== null && a.getTime() === b.getTime();

/** Per weakness, its newest transition into improving, resolved or relapsed that has both rates. */
export function beforeAfterRows(weaknesses: WeaknessDetail[]): BeforeAfterRow[] {
  const rows: BeforeAfterRow[] = [];
  for (const w of weaknesses) {
    const hit = [...w.history]
      .reverse()
      .find(
        (h): h is History & { rateBefore: number; rateAfter: number } =>
          h.kind === "transition" &&
          (CALLED as readonly string[]).includes(h.toState) &&
          fin(h.rateBefore) &&
          fin(h.rateAfter),
      );
    if (!hit) continue;
    const onsetRow = w.history.find(
      (h) => h.kind === "onset" && h.toState === hit.toState && sameTime(h.at, hit.at) && h.onsetAt,
    );
    rows.push({
      seriesKey: w.seriesKey,
      label: w.label,
      explain: seriesGlossary(w.seriesKey),
      state: hit.toState as Called,
      before: hit.rateBefore,
      after: hit.rateAfter,
      onset: onsetRow?.onsetAt
        ? {
            day: isoDay(onsetRow.onsetAt),
            low: onsetRow.onsetLow ? isoDay(onsetRow.onsetLow) : null,
            high: onsetRow.onsetHigh ? isoDay(onsetRow.onsetHigh) : null,
          }
        : null,
      confirmed: hit.at ? isoDay(hit.at) : null,
      afterCounts: hit.counts ?? onsetRow?.counts ?? null,
    });
  }
  const key = (r: BeforeAfterRow) => r.onset?.day ?? r.confirmed ?? "";
  return rows.sort((a, b) => key(b).localeCompare(key(a)));
}

/**
 * "Hung material: 4.1% before, 2.6% after 14 Oct 2026, detected improving" when the change point
 * is stored; otherwise the confirming game, said as such ("…, detected improving on 21 Oct 2026").
 */
export function beforeAfterText(r: BeforeAfterRow): string {
  const head = `${r.label}: ${pct(r.before)} before, ${pct(r.after)} after`;
  if (r.onset) return `${head} ${dayText(r.onset.day)}, detected ${r.state}`;
  const on = r.confirmed ? ` on ${dayText(r.confirmed)}` : "";
  return `${head}, detected ${r.state}${on}`;
}

const W = 240;
const H = 30;
const PAD_L = 48;
const PAD_R = 48;
const MID = 22;

/**
 * A ring for before and a dot for after on the row's own scale from 0, each value labelled above
 * its mark: the lower one ending at it, the higher one starting at it, so the labels never overlap.
 */
function Dumbbell({ before, after }: { before: number; after: number }) {
  const max = Math.max(before, after) * 1.25 || 1;
  const x = (v: number) => PAD_L + (v / max) * (W - PAD_L - PAD_R);
  const lowIsBefore = before <= after;
  const label = (v: number, side: "left" | "right") => (
    <text
      x={side === "left" ? x(v) - 3 : x(v) + 3}
      y={MID - 9}
      textAnchor={side === "left" ? "end" : "start"}
      className="fill-ink-soft text-xs [font-variant-numeric:lining-nums]"
    >
      {pct(v)}
    </text>
  );
  return (
    <svg
      data-dumbbell=""
      aria-hidden="true"
      viewBox={`0 0 ${W} ${H}`}
      width={W}
      height={H}
      className="shrink-0 overflow-visible"
    >
      <line
        x1={PAD_L}
        x2={W - PAD_R}
        y1={MID}
        y2={MID}
        strokeWidth={1}
        className="stroke-rule-strong"
      />
      <line
        x1={PAD_L}
        x2={PAD_L}
        y1={MID - 4}
        y2={MID + 4}
        strokeWidth={1}
        className="stroke-rule-strong"
      />
      <line
        x1={x(before)}
        x2={x(after)}
        y1={MID}
        y2={MID}
        strokeWidth={2}
        className="stroke-ink-soft"
      />
      <circle
        cx={x(before)}
        cy={MID}
        r={4.5}
        strokeWidth={1.75}
        className="fill-sheet stroke-ink-soft"
      />
      <circle cx={x(after)} cy={MID} r={4.5} className="fill-ink" />
      {label(before, lowIsBefore ? "left" : "right")}
      {label(after, lowIsBefore ? "right" : "left")}
    </svg>
  );
}

function StateMark({ state }: { state: Called }) {
  const s = stateStyle[state];
  return (
    <span className="inline-flex items-center gap-1.5 text-sm text-ink-soft">
      <svg aria-hidden="true" width={12} height={12} className="shrink-0">
        <defs>
          <pattern
            id={`ba-${state}`}
            width={4}
            height={4}
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <rect width={4} height={4} fill="var(--color-sheet)" />
            <line x1={0} x2={0} y1={0} y2={4} stroke={s.fill} strokeWidth={2.5} />
            {s.angle === "cross" ? (
              <line x1={0} x2={4} y1={0} y2={0} stroke={s.fill} strokeWidth={2.5} />
            ) : null}
          </pattern>
        </defs>
        <rect
          x={0.5}
          y={s.weight === "band" ? 3 : 0.5}
          width={11}
          height={s.weight === "band" ? 6 : 11}
          rx={2}
          fill={s.pattern === "hatch" ? `url(#ba-${state})` : s.fill}
          stroke={s.fill}
        />
      </svg>
      Detected {s.label.toLowerCase()}
    </span>
  );
}

function Day({ iso }: { iso: string }) {
  return (
    <time dateTime={iso} className="whitespace-nowrap">
      {dayText(iso)}
    </time>
  );
}

/** What each side's rate rests on. Rust stores the after side's counts; the before side's are not. */
function Sides({ r }: { r: BeforeAfterRow }) {
  const c = r.afterCounts;
  const since = r.onset ? (
    <>
      since <Day iso={r.onset.day} />
    </>
  ) : (
    "since the change point"
  );
  const range =
    r.onset?.low && r.onset.high && r.onset.low !== r.onset.high ? (
      <>
        {" "}
        (the change point lies between <Day iso={r.onset.low} /> and <Day iso={r.onset.high} />)
      </>
    ) : null;
  return (
    <ul className="m-0 flex list-none flex-col gap-0.5 p-0 text-sm text-ink-faint [font-variant-numeric:lining-nums]">
      <li>
        After:{" "}
        {c ? (
          <>
            {count.format(c.events)} of {count.format(c.chances)}{" "}
            {c.chances === 1 ? "chance" : "chances"} {since}
          </>
        ) : (
          <>the games {since}; their count isn't stored</>
        )}
        {range}
      </li>
      <li>Before: from the earlier games of that stretch; their count isn't stored yet</li>
    </ul>
  );
}

export function BeforeAfter({ rows }: { rows: BeforeAfterRow[] }) {
  if (rows.length === 0) {
    return (
      <p className="max-w-2xl text-base text-ink-soft">
        No change detected yet. Changes need enough games on both sides to be called.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-3">
      <ol className="m-0 flex list-none flex-col p-0">
        {rows.map((r) => (
          <li
            key={r.seriesKey}
            data-change={r.seriesKey}
            className="flex flex-col gap-3 border-b border-rule py-4 first:pt-0 last:border-b-0 lg:flex-row lg:items-center lg:gap-10"
          >
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <p className="m-0 text-base text-ink [font-variant-numeric:lining-nums]">
                <span className="sr-only">{beforeAfterText(r)}</span>
                <span aria-hidden="true">
                  <span className="font-medium">{r.label}</span>: {pct(r.before)} before,{" "}
                  {pct(r.after)} after
                  {r.onset ? (
                    <>
                      {" "}
                      <Day iso={r.onset.day} />, detected {r.state}
                    </>
                  ) : (
                    <>
                      , detected {r.state}
                      {r.confirmed ? (
                        <>
                          {" "}
                          on <Day iso={r.confirmed} />
                        </>
                      ) : null}
                    </>
                  )}
                </span>
              </p>
              {r.explain ? (
                <span className="flex items-center gap-1">
                  <span className="text-sm text-ink-soft">{r.explain.meaning}</span>
                  <ExplainPopover entry={r.explain} />
                </span>
              ) : null}
              <Sides r={r} />
            </div>
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
              <Dumbbell before={r.before} after={r.after} />
              <StateMark state={r.state} />
            </div>
          </li>
        ))}
      </ol>
      <p className="m-0 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-ink-faint">
        <span className="inline-flex items-center gap-1.5">
          <svg aria-hidden="true" width={10} height={10}>
            <circle cx={5} cy={5} r={3.5} strokeWidth={1.5} className="fill-none stroke-ink-soft" />
          </svg>
          Before
        </span>
        <span className="inline-flex items-center gap-1.5">
          <svg aria-hidden="true" width={10} height={10}>
            <circle cx={5} cy={5} r={4} className="fill-ink" />
          </svg>
          After
        </span>
        <span>Each row on its own scale from 0</span>
      </p>
    </div>
  );
}
