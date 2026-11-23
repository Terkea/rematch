"use client";

import { AreaScoreKey, AreaScoreList } from "@ci/ui/area-score";
import {
  ExemplarBoard,
  ExplainPopover,
  KpiTrendGrid,
  type KpiTrendItem,
  Meaning,
  WeaknessRow,
  WeaknessRowKey,
  type WeaknessRowProps,
  weaknessScaleMax,
} from "@ci/ui/profile";
import { WEAKNESS_GLOSSARY } from "../../../server/profile/glossary";

// The profile kit (ADR 0072) with synthetic numbers: shaped like the R2 read model, never a
// player's data. The two positions are real opening traps; their win chances are made up.

const WEEKS = [
  "2026-08-31",
  "2026-09-07",
  "2026-09-14",
  "2026-09-21",
  "2026-09-28",
  "2026-10-05",
  "2026-10-12",
  "2026-10-19",
  "2026-10-26",
  "2026-11-02",
  "2026-11-09",
  "2026-11-16",
];

/** Points from values, ns and a half-width; a null value is a week with no games. */
function series(values: (number | null)[], ns: number[], half: number) {
  return values.map((value, i) => ({
    week: WEEKS[i] ?? "2026-11-16",
    value,
    low: value === null ? null : value - half * (12 / Math.max(4, ns[i] ?? 4)) ** 0.5,
    high: value === null ? null : value + half * (12 / Math.max(4, ns[i] ?? 4)) ** 0.5,
    n: value === null ? 0 : (ns[i] ?? 0),
  }));
}

const KPIS: KpiTrendItem[] = [
  {
    metric: "accuracy",
    label: "Accuracy",
    unit: "pct",
    points: series(
      [71.2, 72.0, 70.4, null, 73.1, 72.6, 74.0, 73.4, 75.2, 74.1, 75.8, 76.3],
      [11, 9, 12, 0, 10, 3, 12, 11, 13, 9, 12, 14],
      1.6,
    ),
  },
  {
    metric: "blunders_per_game",
    label: "Blunders per game",
    unit: "per_game",
    points: series(
      [1.62, 1.55, 1.7, null, 1.41, 1.5, 1.38, 1.33, 1.29, 1.35, 1.22, 1.18],
      [11, 9, 12, 0, 10, 3, 12, 11, 13, 9, 12, 14],
      0.18,
    ),
  },
  {
    metric: "conversion_rate",
    label: "Converting winning positions",
    unit: "rate",
    points: series(
      [0.62, 0.66, null, null, 0.61, 0.7, 0.68, 0.72, 0.7, 0.74, 0.71, 0.75],
      [8, 7, 0, 0, 6, 2, 9, 8, 7, 6, 8, 9],
      0.07,
    ),
  },
];

const EMPTY_KPI: KpiTrendItem = {
  metric: "resilience_rate",
  label: "Saving worse positions",
  unit: "rate",
  points: WEEKS.slice(-4).map((week) => ({ week, value: null, low: null, high: null, n: 0 })),
};

const BAND = "Players rated 1400–1600";

const ROWS: WeaknessRowProps[] = [
  {
    label: "Leaving pieces undefended",
    state: "active",
    rate: { mean: 0.0421, low: 0.031, high: 0.056 },
    evidence: { events: 38, chances: 902, sufficient: true, needMore: null },
    counts: { event: "pieces left hanging", chance: "moves with a loose piece" },
    band: { median: 0.021, label: BAND },
    href: "#profile",
    trend: [0.052, 0.049, 0.05, null, 0.046, 0.047, 0.044, 0.043, 0.045, 0.042],
    onset: "2026-09-14",
  },
  {
    label: "Rook endgames",
    state: "improving",
    rate: { mean: 0.118, low: 0.084, high: 0.152 },
    evidence: { events: 31, chances: 263, sufficient: true, needMore: null },
    counts: { event: "errors", chance: "rook-endgame moves" },
    band: { median: 0.11, label: BAND },
    href: "#profile",
    trend: [0.17, 0.16, 0.15, 0.14, null, null, 0.13, 0.12, 0.12],
    onset: "2026-08-20",
  },
  {
    label: "Missed forks",
    state: "emerging",
    rate: { mean: 0.09, low: 0.04, high: 0.17 },
    evidence: { events: 4, chances: 40, sufficient: false, needMore: 180 },
    counts: { event: "missed", chance: "fork chances" },
    band: { median: 0.06, label: BAND },
    href: "#profile",
    trend: [null, 0.1, null, 0.08],
    onset: null,
  },
  {
    label: "Blunders on a low clock",
    state: "resolved",
    rate: { mean: 0.018, low: 0.011, high: 0.027 },
    evidence: { events: 14, chances: 780, sufficient: true, needMore: null },
    counts: { event: "blunders", chance: "moves under 30 seconds" },
    band: { median: null },
    href: "#profile",
    trend: [0.04, 0.035, 0.03, 0.026, 0.022, 0.02, 0.019, 0.018],
    onset: "2026-07-02",
  },
];

/** The entry the owner asked about ("what is conversion failure?"). */
const CONVERSION = WEAKNESS_GLOSSARY.W5;

export function ProfileSection() {
  const scaleMax = weaknessScaleMax(ROWS);
  return (
    <section
      id="profile"
      data-section="profile"
      aria-labelledby="profile-title"
      className="flex flex-col gap-10 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="profile-title" className="font-serif text-2xl font-semibold">
          Profile
        </h2>
        <p className="text-sm text-ink-soft">
          The pieces of the coach pages: a weakness as a row, KPI trends as small multiples on one
          time axis, and example positions. The numbers here are synthetic.
        </p>
      </div>

      <div data-specimen="explain" className="flex flex-col gap-3">
        <h3 className="font-serif text-xl font-semibold">Explanations</h3>
        <p className="m-0 max-w-prose text-sm text-ink-soft">
          Every weakness, measure and area name carries its one-sentence meaning in muted text, and
          an "i" that opens how it is measured, why it matters and what to try. It opens on click or
          tap, never on hover alone, and Escape or a click outside closes it. The words come from
          the glossary, never from the component.
        </p>
        {CONVERSION ? (
          <div className="flex flex-col gap-1">
            <span className="flex items-center gap-1">
              <span className="font-medium text-ink">{CONVERSION.name}</span>
              <ExplainPopover entry={CONVERSION} />
            </span>
            <Meaning entry={CONVERSION} />
          </div>
        ) : null}
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="font-serif text-xl font-semibold">Weakness rows</h3>
        <WeaknessRowKey bandLabel={BAND} />
        <ul className="m-0 flex list-none flex-col p-0">
          {ROWS.map((row) => (
            <li key={row.label}>
              <WeaknessRow {...row} scaleMax={scaleMax} />
            </li>
          ))}
        </ul>
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="font-serif text-xl font-semibold">KPI trends</h3>
        <KpiTrendGrid
          items={[...KPIS, EMPTY_KPI]}
          range={{ from: WEEKS[0] ?? "2026-08-31", to: WEEKS[11] ?? "2026-11-16" }}
          events={[
            { date: "2026-09-21", label: "Coaching report" },
            { date: "2026-10-26", label: "Coaching report" },
          ]}
          minN={5}
          headingLevel={4}
        />
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="font-serif text-xl font-semibold">Example positions</h3>
        <div className="flex flex-wrap gap-x-10 gap-y-8">
          <ExemplarBoard
            className="[--board-size:min(100vw_-_4rem,22rem)]" // ban-lint-allow TOK2: a styleguide demo size for two boards side by side
            fen="r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4"
            played={{ san: "Nxe5", uci: "f3e5" }}
            best={{ san: "Nxd4", uci: "f3d4" }}
            refutation="d8g5"
            caption={{ judgement: "mistake", winBefore: 52.4, winAfter: 36.1 }}
          />
          <ExemplarBoard
            className="[--board-size:min(100vw_-_4rem,22rem)]" // ban-lint-allow TOK2: a styleguide demo size for two boards side by side
            fen="r1bqkb1r/pppp1ppp/2n2n2/4p1N1/2B1P3/8/PPPP1PPP/RNBQK2R b KQkq - 5 4"
            played={{ san: "h6", uci: "h7h6" }}
            best={{ san: "d5", uci: "d7d5" }}
            refutation="g5f7"
            caption={{ judgement: "blunder", winBefore: 47.8, winAfter: 21.5 }}
          />
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="font-serif text-xl font-semibold">
          Area scores without a measured next band
        </h3>
        <AreaScoreKey next={false} interval />
        <AreaScoreList
          items={[
            {
              area: "endgame",
              score: 62,
              band: { median: 50 },
              interval: { low: 55, high: 68, level: 0.8 },
              basis: { games: 22, timeClass: "rapid" },
            },
            {
              area: "tactics",
              score: 44,
              band: { median: 50 },
              interval: null,
              basis: { games: 9, timeClass: "rapid" },
            },
          ]}
        />
      </div>
    </section>
  );
}
