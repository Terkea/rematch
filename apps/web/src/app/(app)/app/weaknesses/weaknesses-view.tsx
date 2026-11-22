import { Button } from "@ci/ui/primitives";
import { WeaknessRow, WeaknessRowKey, weaknessScaleMax } from "@ci/ui/profile";
import Link from "next/link";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { MorphLink } from "../../../../components/shared-morph";
import type { CoachScope, DifficultyStatus, WeaknessSummary } from "../../../../server/profile";
import { EmptySheet } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";
import { basisText, weaknessHref } from "./queries";

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

export function groupWeaknesses(list: WeaknessSummary[]) {
  return GROUPS.map((g) => ({
    ...g,
    rows: list.filter((w) => (g.states as readonly string[]).includes(w.state)),
  }));
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

export function WeaknessesView({
  scope,
  weaknesses,
  trends,
  difficulty,
}: {
  scope: CoachScope | null;
  weaknesses: WeaknessSummary[];
  trends: Map<string, (number | null)[]>;
  difficulty: DifficultyStatus | null;
}) {
  const groups = groupWeaknesses(weaknesses).filter((g) => g.rows.length > 0);
  // One interval scale for the whole page, so a row's range compares with every other row's.
  const scaleMax = weaknessScaleMax(weaknesses);
  return (
    <div className="flex flex-col gap-10">
      <div className="flex flex-col gap-5">
        <WeaknessesHeader scope={scope} difficulty={difficulty} />
        {scope ? <CoachScopeSwitch scope={scope} basePath="/app/weaknesses" /> : null}
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
          <WeaknessRowKey />
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
                        band={{ median: w.band.median }}
                        href={weaknessHref(w.seriesKey, scope)}
                        trend={trends.get(w.seriesKey) ?? []}
                        onset={w.onset ? w.onset.at.toISOString().slice(0, 10) : null}
                        scaleMax={scaleMax}
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
