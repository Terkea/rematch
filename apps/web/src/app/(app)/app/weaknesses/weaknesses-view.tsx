import { Button } from "@ci/ui/primitives";
import { NOT_COMPARED_LABEL, WeaknessRow, WeaknessRowKey, weaknessScaleMax } from "@ci/ui/profile";
import Link from "next/link";
import type { ReactNode } from "react";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { MorphLink } from "../../../../components/shared-morph";
import type { CoachScope, DifficultyStatus, WeaknessSummary } from "../../../../server/profile";
import {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  benchmarkGroupLabel,
  benchmarkLabel,
} from "../../../../server/profile/benchmark-text";
import { seriesGlossary } from "../../../../server/profile/glossary";
import { ESTIMATE_WINDOW_TEXT } from "../../../../server/profile/window";
import { EmptySheet } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";
import { basisText, WEEKLY_MIN_CHANCES, weaknessHref } from "./queries";

// `/app/weaknesses` (M5b Task 7): every measured weakness of the scope, grouped by what it is
// doing to the player's games, each group by priority. Each row opens its detail, and the row
// morphs into that page's header (choreography 4).

export const EMPTY_TEXT = "No weaknesses measured yet. They appear after about 20 analysed games.";

export const GROUPS = [
  {
    id: "costing",
    title: "Costing you games",
    lede: "Detected as active, emerging or back again, most pressing first.",
    states: ["active", "emerging", "relapsed"],
  },
  {
    id: "better",
    title: "Getting better",
    lede: "Detected as improving, not yet settled enough to call resolved.",
    states: ["improving"],
  },
  {
    id: "control",
    title: "Under control",
    lede: "Detected as resolved, or not an issue in your games.",
    states: ["resolved", "not_an_issue"],
  },
] as const;

/** Rows below their evidence minimum: never under a group that states what they are doing. */
export const EARLY_GROUP = {
  id: "early",
  title: "Early reads",
  lede: "Not enough chances yet to tell what these are doing. Each says how many more it needs.",
} as const;

/**
 * Rows with no rating-band benchmark (bug hunt I1): Rust stores them `not_an_issue`, which is no
 * claim, so they never sit under a group that says what they are doing.
 */
export const UNCOMPARED_GROUP = {
  id: "uncompared",
  title: NOT_COMPARED_LABEL,
  lede: "No benchmark for players at your level yet. Each shows your own rate, its range and its chances.",
} as const;

export function groupWeaknesses(list: WeaknessSummary[]) {
  const settled = list.filter((w) => w.evidence.sufficient);
  const compared = settled.filter((w) => w.compared);
  return [
    ...GROUPS.map((g) => ({
      id: g.id as string,
      title: g.title as string,
      lede: g.lede as string,
      rows: compared.filter((w) => (g.states as readonly string[]).includes(w.state)),
    })),
    { ...UNCOMPARED_GROUP, rows: settled.filter((w) => !w.compared) },
    { ...EARLY_GROUP, rows: list.filter((w) => !w.evidence.sufficient) },
  ];
}

const count = new Intl.NumberFormat("en-GB");

/** "Difficulty adjustment is waiting for Maia on 112 of 3,040 positions.", or null at zero. */
export function difficultyText(d: DifficultyStatus): string | null {
  if (d.pendingMaia <= 0 || d.positions <= 0) return null;
  return `Difficulty adjustment is waiting for Maia on ${count.format(d.pendingMaia)} of ${count.format(d.positions)} positions.`;
}

/** A valid `view-transition-name` per series ("weakness-W3-shallow"). */
export const morphName = (seriesKey: string) =>
  `weakness-${seriesKey.replace(/[^A-Za-z0-9_-]/g, "-")}`;

export function WeaknessesHeader({
  scope,
  difficulty,
}: {
  scope: CoachScope | null;
  difficulty: DifficultyStatus | null;
}) {
  const waiting = difficulty ? difficultyText(difficulty) : null;
  return (
    <header className="flex max-w-2xl flex-col gap-2">
      <h1 className="font-serif text-3xl font-semibold text-ink">Weaknesses</h1>
      {scope ? (
        <p className="text-lg text-ink-soft [font-variant-numeric:lining-nums]">
          {basisText(scope)}
        </p>
      ) : null}
      {waiting ? (
        <p className="text-sm text-ink-soft [font-variant-numeric:lining-nums]">{waiting}</p>
      ) : null}
    </header>
  );
}

/** What the compared rows were compared with, and its bias; null without a peer benchmark. */
export function BenchmarkNote({ benchmark }: { benchmark: Benchmark | null }) {
  const label = benchmarkLabel(benchmark);
  if (label === null) return null;
  return (
    <p data-benchmark="peers" className="text-sm text-ink-soft [font-variant-numeric:lining-nums]">
      {label}. {BENCHMARK_BIAS_TEXT}
    </p>
  );
}

export function WeaknessesView({
  scope,
  weaknesses,
  trends,
  difficulty,
  benchmark = null,
  filterBar,
  accountSwitch = true,
}: {
  scope: CoachScope | null;
  weaknesses: WeaknessSummary[];
  trends: Map<string, (number | null)[]>;
  difficulty: DifficultyStatus | null;
  benchmark?: Benchmark | null;
  /** The shared filter bar (T3); it holds the time control, so the scope switch then only picks the account. */
  filterBar?: ReactNode;
  /** Whether the scope switch is still needed (more than one linked account). */
  accountSwitch?: boolean;
}) {
  const group = benchmarkGroupLabel(benchmark);
  const groups = groupWeaknesses(weaknesses).filter((g) => g.rows.length > 0);
  // One interval scale for the whole page, so a row's range compares with every other row's.
  const scaleMax = weaknessScaleMax(weaknesses);
  return (
    <div className="flex flex-col gap-10">
      <div className="flex flex-col gap-5">
        <WeaknessesHeader scope={scope} difficulty={difficulty} />
        {scope && (accountSwitch || !filterBar) ? (
          <CoachScopeSwitch scope={scope} basePath="/app/weaknesses" />
        ) : null}
        {filterBar}
      </div>

      {scope === null || weaknesses.length === 0 ? (
        <EmptySheet
          action={
            <Button asChild variant="outline">
              <Link href="/app/games">Your games</Link>
            </Button>
          }
        >
          {EMPTY_TEXT}
        </EmptySheet>
      ) : (
        <>
          <div className="flex flex-col gap-2">
            <WeaknessRowKey />
            <BenchmarkNote benchmark={benchmark} />
            <p className="text-sm text-ink-soft">
              Weekly rate lines leave out weeks with fewer than {WEEKLY_MIN_CHANCES} chances, and
              weeks with none, as gaps.
            </p>
          </div>
          {groups.map((g) => (
            <section
              key={g.id}
              aria-labelledby={`group-${g.id}`}
              data-group={g.id}
              className="flex flex-col gap-4"
            >
              <SectionHead id={`group-${g.id}`} title={g.title}>
                <p className="text-sm text-ink-soft">{g.lede}</p>
              </SectionHead>
              <ul
                data-surface="sheet"
                className="m-0 flex list-none flex-col divide-y divide-rule rounded-sheet bg-sheet p-0 px-2 py-1 md:px-3"
              >
                {g.rows.map((w) => (
                  <li key={w.seriesKey}>
                    <MorphLink name={morphName(w.seriesKey)}>
                      <WeaknessRow
                        label={w.label}
                        state={w.state}
                        rate={w.rate}
                        evidence={w.evidence}
                        band={
                          group
                            ? { median: w.band.median, label: group }
                            : { median: w.band.median }
                        }
                        compared={w.compared}
                        window={ESTIMATE_WINDOW_TEXT}
                        href={weaknessHref(w.seriesKey, scope)}
                        trend={trends.get(w.seriesKey) ?? []}
                        onset={w.onset ? w.onset.at.toISOString().slice(0, 10) : null}
                        scaleMax={scaleMax}
                        explain={seriesGlossary(w.seriesKey)}
                      />
                    </MorphLink>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </>
      )}
    </div>
  );
}
