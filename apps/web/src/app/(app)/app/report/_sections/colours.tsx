"use client";

import type {
  ByColor,
  ColorSummary,
  InputBenchmark,
} from "../../../../../server/coaching/types";
import { BENCHMARK_BIAS_TEXT } from "../../../../../server/profile/benchmark-text";
import { seriesGlossary } from "../../../../../server/profile/glossary";
import {
  COLOUR_NAME,
  type Colour,
  type ColourMetricKind,
  colourCompareText,
  colourMetricText,
  colourRecordText,
  colourWeaknessPeerText,
  colourWeaknessRateText,
} from "./colours-text";
import { ReportSection } from "./shell";

const ROWS: { kind: ColourMetricKind; label: string; key: "score" | "accuracy" | "blundersPerGame" }[] =
  [
    { kind: "score", label: "Score", key: "score" },
    { kind: "accuracy", label: "Accuracy", key: "accuracy" },
    { kind: "blunders", label: "Blunders per game", key: "blundersPerGame" },
  ];

function Side({
  colour,
  s,
  benchmark,
}: {
  colour: Colour;
  s: ColorSummary;
  benchmark: InputBenchmark | undefined;
}) {
  const name = COLOUR_NAME[colour];
  return (
    <div
      data-surface="sheet"
      className="flex min-w-0 flex-col gap-4 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
    >
      <div className="flex flex-col gap-1">
        <h3 className="m-0 font-serif text-lg font-semibold text-ink">As {name}</h3>
        <p className="m-0 text-sm text-ink-soft">{colourRecordText(s)}</p>
      </div>
      {s.sufficient ? null : (
        <p className="m-0 text-sm text-ink-soft">
          Need {s.needMore ?? 1} more {s.needMore === 1 ? "game" : "games"} with {name} before
          comparing.
        </p>
      )}
      <dl className="m-0 grid grid-cols-1 gap-3">
        {ROWS.map((r) => {
          const t = colourMetricText(s[r.key], r.kind);
          return (
            <div key={r.key} className="flex flex-wrap items-baseline justify-between gap-x-4">
              <dt className="text-sm text-ink-soft">{r.label}</dt>
              <dd className="m-0 flex items-baseline gap-2">
                <span className="font-serif text-xl text-ink tabular-nums">{t.value}</span>
                <span className="text-sm text-ink-faint">{t.detail}</span>
              </dd>
            </div>
          );
        })}
      </dl>
      {s.weaknesses.length > 0 ? (
        <div className="flex flex-col gap-2">
          <h4 className="m-0 text-base font-medium text-ink">What costs you most as {name}</h4>
          <ul className="m-0 flex list-none flex-col gap-3 p-0">
            {s.weaknesses.map((w) => (
              <li key={w.seriesKey} className="flex flex-col gap-1 text-sm">
                <span className="text-ink">{seriesGlossary(w.seriesKey)?.name ?? w.label}</span>
                <span className="text-ink-soft">{colourWeaknessRateText(w)}</span>
                <span className="text-ink-faint">{colourWeaknessPeerText(w, colour, benchmark)}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

/**
 * White vs Black (ADR 0084): the stored report's colour split, side by side. Each colour states
 * its own games; below the minimum it shows a dash and the shortfall. The comparison names a
 * direction only when the two 80% ranges separate. Nothing for a report from before the split.
 */
export function Colours({
  byColor,
  benchmark,
  order,
}: {
  byColor: ByColor | undefined;
  benchmark: InputBenchmark | undefined;
  order: number;
}) {
  if (!byColor) return null;
  const { white, black } = byColor;
  const compare = [
    colourCompareText("score", white.score, black.score),
    colourCompareText("accuracy", white.accuracy, black.accuracy),
    colourCompareText("blunders", white.blundersPerGame, black.blundersPerGame, false),
  ].filter((t): t is string => t !== null);
  const unique = [...new Set(compare)];
  const peers = [...white.weaknesses, ...black.weaknesses].some((w) => w.peerRate !== null);
  return (
    <ReportSection
      id="report-colours"
      title="White and Black"
      order={order}
      intro={
        <p className="m-0 max-w-prose text-sm text-ink-soft">
          Your most recent games with each colour, up to 50 per colour.
        </p>
      }
    >
      {unique.length > 0 ? (
        <ul className="m-0 flex list-none flex-col gap-1 p-0 font-serif text-lg text-ink">
          {unique.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      ) : null}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Side colour="white" s={white} benchmark={benchmark} />
        <Side colour="black" s={black} benchmark={benchmark} />
      </div>
      {peers ? <p className="m-0 text-sm text-ink-faint">{BENCHMARK_BIAS_TEXT}</p> : null}
    </ReportSection>
  );
}
