"use client";

import {
  AccuracyTrend,
  type AccuracyTrendPoint,
  type AccuracyTrendRolling,
  JudgementColumns,
  type JudgementColumnsGame,
  ResultSplit,
  type ResultSplitRow,
  type TrendResult,
} from "@ci/ui/charts";
import { type GameFixture, games, kpiFixtures } from "@ci/ui/fixtures";
import { useState } from "react";

// The overview charts (ADR 0050). The judgement columns and the first result row use the three
// real fixture games. Everything else is synthetic, shaped from kpiFixtures, and labelled so on
// the page: the styleguide is the only place synthetic rows may show (B15).

const real = [games.middlegameBlunder, games.endgameGrind, games.shortAttack];

const outcome = (g: GameFixture): "wins" | "draws" | "losses" => {
  if (g.result === "1/2-1/2") return "draws";
  const whiteWon = g.result === "1-0";
  return whiteWon === (g.playerColor === "white") ? "wins" : "losses";
};

/** The player's own judged moves in a real fixture game. */
const realColumns: JudgementColumnsGame[] = real.map((g) => {
  const mine = g.plies.filter((p) => p.color === g.playerColor);
  const count = (j: string) => mine.filter((p) => p.judgement === j).length;
  return {
    id: g.id,
    label: `${g.white.name} v ${g.black.name}`,
    href: g.source,
    blunders: count("blunder"),
    mistakes: count("mistake"),
    inaccuracies: count("inaccuracy"),
  };
});

const realSplit: ResultSplitRow = real.reduce(
  (row, g) => {
    row[outcome(g)] += 1;
    return row;
  },
  { key: "fixtures", label: "Fixture games", wins: 0, draws: 0, losses: 0 },
);

/** mulberry32: the same synthetic games on every render, server and client. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const accuracyKpi = kpiFixtures.items[0];
const DAY = 86_400_000;
const START = Date.UTC(2025, 8, 1);
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const dayText = (t: number) => {
  const d = new Date(t);
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
};

/**
 * 500 synthetic games over 14 months, drifting from the accuracy KPI's first trend value to its
 * current value, with a two-month break in the middle so the line visibly breaks.
 */
function syntheticTrend(): { points: AccuracyTrendPoint[]; rolling: AccuracyTrendRolling[] } {
  const next = rng(50);
  const from = (accuracyKpi?.trend?.[0] ?? 0.69) * 100;
  const to = (accuracyKpi?.value ?? 0.742) * 100;
  const points: AccuracyTrendPoint[] = [];
  let t = START;
  for (let i = 0; i < 500; i++) {
    t += next() * 0.8 * DAY;
    if (i === 260) t += 62 * DAY;
    const mean = from + ((to - from) * i) / 499;
    const accuracy = Math.min(99, Math.max(28, mean + (next() + next() + next() - 1.5) * 24));
    const roll = next();
    const result: TrendResult =
      roll < 0.02
        ? "unfinished"
        : roll < 0.12
          ? "draw"
          : roll < 0.12 + accuracy / 150
            ? "win"
            : "loss";
    points.push({
      id: `s${i}`,
      date: new Date(t).toISOString(),
      accuracy: Math.round(accuracy * 10) / 10,
      result,
      caption: `Synthetic game ${i + 1}, ${dayText(t)}, ${result}, accuracy ${accuracy.toFixed(1)}%`,
      href: "#charts",
    });
  }
  // A trailing 20-game window, shown once it holds 10 games played within 30 days of each other,
  // so the break leaves a real gap. The band is the mean ± z(0.8) standard errors, clamped.
  const rolling: AccuracyTrendRolling[] = [];
  points.forEach((p, i) => {
    const end = new Date(p.date).getTime();
    const window = points
      .slice(Math.max(0, i - 19), i + 1)
      .filter((q) => end - new Date(q.date).getTime() <= 30 * DAY)
      .map((q) => q.accuracy);
    if (window.length < 10) return;
    const mean = window.reduce((a, b) => a + b, 0) / window.length;
    const sd = Math.sqrt(window.reduce((a, b) => a + (b - mean) ** 2, 0) / (window.length - 1));
    const half = (1.2815515655446004 * sd) / Math.sqrt(window.length);
    // As Task 2's rolling mean: clamped to 0 to 100, and no band at all when there is no spread.
    const band = half > 0;
    rolling.push({
      id: p.id,
      mean,
      low: band ? Math.max(0, mean - half) : null,
      high: band ? Math.min(100, mean + half) : null,
    });
  });
  return { points, rolling };
}

const trend = syntheticTrend();

/** 40 synthetic games' judgements, oldest first, a few of them clean. */
const syntheticColumns: JudgementColumnsGame[] = (() => {
  const next = rng(40);
  const pick = (p: number, max: number) => {
    let n = 0;
    while (n < max && next() < p) n++;
    return n;
  };
  return Array.from({ length: 40 }, (_, i) => ({
    id: `c${i}`,
    label: `Synthetic game ${i + 1}`,
    href: "#charts",
    blunders: pick(0.35, 4),
    mistakes: pick(0.45, 4),
    inaccuracies: pick(0.6, 6),
  }));
})();

/** Rapid matches the KPI fixtures' 38 rapid games. */
const syntheticSplit: ResultSplitRow[] = [
  { key: "rapid", label: "Rapid", wins: 21, draws: 6, losses: 11 },
  { key: "blitz", label: "Blitz", wins: 1, draws: 0, losses: 1 },
  { key: "daily", label: "Daily", wins: 0, draws: 0, losses: 0 },
];

function Example({
  title,
  note,
  children,
}: {
  title: string;
  note: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <h3 className="font-serif text-lg font-semibold">{title}</h3>
        <p className="text-sm text-ink-soft">{note}</p>
      </div>
      {children}
    </div>
  );
}

export function ChartsSection() {
  const [run, setRun] = useState(0);
  return (
    <section
      id="charts"
      data-section="charts"
      aria-labelledby="charts-title"
      className="flex flex-col gap-10 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="charts-title" className="font-serif text-2xl font-semibold">
            Overview charts
          </h2>
          <p className="text-sm text-ink-soft">
            How a player&apos;s games add up. A result is told by shape as well as tone, and a
            mistake by pattern as well as red, so nothing rests on colour alone. Red is only ever
            the player&apos;s own mistakes.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setRun((n) => n + 1)}
          className="rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Replay draw
        </button>
      </div>

      <Example
        title="Accuracy over time"
        note="Synthetic example: 500 made-up games. Each dot is a game. The line is the average over the last 20 games, with an 80% band, and it breaks where there were too few recent games. Open a dot to review its game."
      >
        <AccuracyTrend
          key={run}
          drawIn={run > 0}
          points={trend.points}
          rolling={trend.rolling}
          label="Accuracy over time, 500 synthetic games"
        />
      </Example>

      <Example
        title="Accuracy with one game"
        note="With fewer than two games there is no trend to draw."
      >
        <AccuracyTrend
          points={trend.points.slice(0, 1)}
          rolling={[]}
          label="Accuracy over time, one game"
        />
      </Example>

      <Example
        title="Mistakes per game"
        note="The three real fixture games, the player's moves only. Each column links to its game."
      >
        <JudgementColumns games={realColumns} label="Mistakes in the three fixture games" />
      </Example>

      <Example
        title="Mistakes in the last 40 games"
        note="Synthetic example: 40 made-up games. On a narrow screen the chart scrolls on its own."
      >
        <JudgementColumns games={syntheticColumns} label="Mistakes in 40 synthetic games" />
      </Example>

      <Example
        title="Mistakes with no games"
        note="With no analysed games there is nothing to count."
      >
        <JudgementColumns games={[]} label="Mistakes, no games" />
      </Example>

      <Example
        title="Results by time control"
        note="The first row is the three real fixture games. The others are synthetic, and a time control with no games is left out."
      >
        <ResultSplit rows={[realSplit, ...syntheticSplit]} />
      </Example>
    </section>
  );
}
