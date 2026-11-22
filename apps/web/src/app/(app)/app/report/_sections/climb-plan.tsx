"use client";

import type { AiDisclosureData } from "@ci/ui/coach";
import { weaknessRateText } from "@ci/ui/profile";
import type {
  ClimbPlanSection,
  ComputedReport,
  DrillType,
} from "../../../../../server/coaching/types";
import { CoachText, ReportSection, SectionNote, type SectionStatus } from "./shell";

/** Rust `DrillType` by name. Drills are not built yet, so the plan names them and links nowhere. */
export const DRILL_NAMES: Record<DrillType, string> = {
  own_mistake_puzzles: "Puzzles from your own mistakes",
  safe_unsafe: "Safe or unsafe",
  themed_puzzles: "Themed puzzles",
  long_calculation: "Long calculation",
  mate_patterns: "Mate patterns",
  convert_vs_engine: "Convert a winning position against the engine",
  defend_vs_engine: "Defend a worse position against the engine",
  opening_trainer: "Opening trainer",
  opening_trap_puzzles: "Opening trap puzzles",
  tablebase_drills: "Tablebase endgames",
  clock_pressure: "Clock pressure",
  critical_or_not: "Critical or not",
  blitz_safety: "Blitz safety checks",
  model_games: "Model games",
  guess_the_move: "Guess the move",
  session_rules: "Session rules",
  slow_games: "Slow games",
  own_game_analysis: "Analyse your own games",
};

const MAX_FOCUSES = 3;
/** One focus reads at prose width; two or three sit side by side from `lg`. */
const COLUMNS: Record<number, string> = {
  1: "max-w-prose",
  2: "lg:grid-cols-2",
  3: "lg:grid-cols-3",
};

/**
 * The climb plan: at most three focuses, each with the coach's why, a habit and the drills by
 * name. There is no "train this" button: drills are out of M5b's scope (ADR 0073).
 */
export function ClimbPlan({
  computed,
  content,
  status,
  disclosure,
  order,
}: {
  computed: ComputedReport;
  content: ClimbPlanSection | null;
  status: SectionStatus;
  disclosure: AiDisclosureData | null;
  order: number;
}) {
  const ready = status === "ready" && content !== null;
  const weakness = (id: string) => computed.weaknesses.find((w) => w.id === id) ?? null;
  const label = (id: string) => weakness(id)?.label ?? null;
  const ids = ready
    ? content.focuses.map((f) => f.weaknessId)
    : computed.focuses.filter((id) => label(id) !== null);
  const focuses = ids.slice(0, MAX_FOCUSES);
  return (
    <ReportSection
      id="report-climb-plan"
      title="Your climb plan"
      order={order}
      disclosure={ready ? disclosure : null}
    >
      {ready ? <CoachText item={content.intro} /> : <SectionNote status={status} />}
      {focuses.length === 0 ? (
        <p className="m-0 text-sm text-ink-soft">No focus has been picked for this report.</p>
      ) : (
        <ol
          className={`m-0 grid list-none grid-cols-1 gap-x-10 gap-y-8 p-0 ${COLUMNS[focuses.length] ?? ""}`}
        >
          {focuses.map((id, i) => {
            const f = ready ? content.focuses.find((x) => x.weaknessId === id) : undefined;
            return (
              <li key={id} className="flex flex-col gap-2">
                <h3 className="m-0 flex items-baseline gap-2 font-serif text-lg font-semibold text-ink">
                  <span className="text-ink-faint" style={{ fontVariantNumeric: "lining-nums" }}>
                    {i + 1}
                  </span>
                  {label(id) ?? "Focus"}
                </h3>
                {(() => {
                  const w = weakness(id);
                  return w ? (
                    <p
                      className="m-0 text-sm text-ink-soft"
                      style={{ fontVariantNumeric: "lining-nums" }}
                    >
                      {weaknessRateText(
                        { mean: w.posteriorRate, low: w.intervalLow, high: w.intervalHigh },
                        { events: w.events, chances: w.opportunities },
                      )}{" "}
                      chances.
                    </p>
                  ) : null;
                })()}
                {f ? (
                  <>
                    <CoachText item={f.why} className="text-base" />
                    {f.habit ? (
                      <div className="flex flex-col gap-0.5">
                        <span className="text-sm font-medium text-ink">Habit</span>
                        <CoachText item={f.habit} className="text-base" />
                      </div>
                    ) : null}
                    {f.drills.length > 0 ? (
                      <div className="flex flex-col gap-0.5">
                        <span className="text-sm font-medium text-ink">Drills</span>
                        <ul className="m-0 flex list-disc flex-col gap-0.5 pl-5 text-sm text-ink-soft">
                          {f.drills.map((d) => (
                            <li key={d.drillType}>{DRILL_NAMES[d.drillType] ?? "A drill"}</li>
                          ))}
                        </ul>
                      </div>
                    ) : null}
                  </>
                ) : null}
              </li>
            );
          })}
        </ol>
      )}
      {ready && content.weeklyRhythm ? (
        <div className="flex flex-col gap-0.5">
          <span className="text-sm font-medium text-ink">Your weekly rhythm</span>
          <CoachText item={content.weeklyRhythm} className="text-base" />
        </div>
      ) : null}
    </ReportSection>
  );
}
