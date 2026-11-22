import { stateStyle } from "@ci/ui/timeline";
import type { WeaknessDetail } from "../../../../server/profile";

// Before and after (spec §10a.4): the rates either side of a detected change, straight from the
// weakness's own transition row (`weakness_state_history.rate_before` / `rate_after`). The words
// follow §12a: a change is "detected", never "fixed", and nothing names training as its cause.

const CALLED = ["improving", "resolved", "relapsed"] as const;
type Called = (typeof CALLED)[number];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const count = new Intl.NumberFormat("en-GB");

export type BeforeAfterRow = {
  seriesKey: string;
  label: string;
  state: Called;
  before: number;
  after: number;
  /** The change point (the onset when stored, else the transition's game), ISO day. */
  day: string | null;
  chances: number;
};

const fin = (v: number | null): v is number => v !== null && Number.isFinite(v);

export function isoDay(d: Date): string {
  return d.toISOString().slice(0, 10);
}

export function dayWords(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return `${d} ${MONTHS[(m ?? 1) - 1]} ${y}`;
}

const pct = (share: number) => `${(share * 100).toFixed(1)}%`;

/** Per weakness, its newest transition into improving, resolved or relapsed that has both rates. */
export function beforeAfterRows(weaknesses: WeaknessDetail[]): BeforeAfterRow[] {
  const rows: BeforeAfterRow[] = [];
  for (const w of weaknesses) {
    const hit = [...w.history]
      .reverse()
      .find(
        (h) =>
          h.kind === "transition" &&
          (CALLED as readonly string[]).includes(h.toState) &&
          fin(h.rateBefore) &&
          fin(h.rateAfter),
      );
    if (!hit || !fin(hit.rateBefore) || !fin(hit.rateAfter)) continue;
    const at = hit.onsetAt ?? hit.at;
    rows.push({
      seriesKey: w.seriesKey,
      label: w.label,
      state: hit.toState as Called,
      before: hit.rateBefore,
      after: hit.rateAfter,
      day: at ? isoDay(at) : null,
      chances: w.evidence.chances,
    });
  }
  return rows.sort((a, b) => (b.day ?? "").localeCompare(a.day ?? ""));
}

/** "Hung material: 4.1% before, 2.6% after 14 Oct 2026, detected improving". */
export function beforeAfterText(r: BeforeAfterRow): string {
  const when = r.day ? ` ${dayWords(r.day)}` : "";
  return `${r.label}: ${pct(r.before)} before, ${pct(r.after)} after${when}, detected ${r.state}`;
}

const W = 160;
const H = 14;
const PAD = 6;

/** A ring for before and a dot for after on one shared scale, joined by a rule. */
function Dumbbell({ before, after, max }: { before: number; after: number; max: number }) {
  const x = (v: number) => PAD + (max > 0 ? v / max : 0) * (W - 2 * PAD);
  return (
    <svg
      aria-hidden="true"
      viewBox={`0 0 ${W} ${H}`}
      width={W}
      height={H}
      className="shrink-0 overflow-visible"
    >
      <line x1={PAD} x2={W - PAD} y1={H / 2} y2={H / 2} strokeWidth={1} className="stroke-rule" />
      <line
        x1={x(before)}
        x2={x(after)}
        y1={H / 2}
        y2={H / 2}
        strokeWidth={2}
        className="stroke-ink-soft"
      />
      <circle
        cx={x(before)}
        cy={H / 2}
        r={4}
        strokeWidth={1.75}
        className="fill-sheet stroke-ink-soft"
      />
      <circle cx={x(after)} cy={H / 2} r={4} className="fill-ink" />
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

export function BeforeAfter({ rows }: { rows: BeforeAfterRow[] }) {
  if (rows.length === 0) {
    return (
      <p className="max-w-2xl text-base text-ink-soft">
        No change detected yet. Changes need enough games on both sides to be called.
      </p>
    );
  }
  const max = Math.max(...rows.flatMap((r) => [r.before, r.after])) * 1.15;
  return (
    <div className="flex flex-col gap-3">
      <ul className="m-0 flex list-none flex-col p-0">
        {rows.map((r) => (
          <li
            key={r.seriesKey}
            className="flex flex-col gap-2 border-b border-rule py-4 first:pt-0 last:border-b-0 md:flex-row md:items-center md:gap-6"
          >
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <p className="m-0 text-base text-ink [font-variant-numeric:lining-nums]">
                <span className="sr-only">{beforeAfterText(r)}</span>
                <span aria-hidden="true">
                  <span className="font-medium">{r.label}</span>: {pct(r.before)} before,{" "}
                  {pct(r.after)} after
                  {r.day ? (
                    <>
                      {" "}
                      <time dateTime={r.day} className="whitespace-nowrap">
                        {dayWords(r.day)}
                      </time>
                    </>
                  ) : null}
                  , detected {r.state}
                </span>
              </p>
              <p className="m-0 text-sm text-ink-faint">
                Rates either side of the change point, from {count.format(r.chances)}{" "}
                {r.chances === 1 ? "chance" : "chances"} in all
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
              <Dumbbell before={r.before} after={r.after} max={max} />
              <StateMark state={r.state} />
            </div>
          </li>
        ))}
      </ul>
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
        <span>On one scale from 0 for every row</span>
      </p>
    </div>
  );
}
