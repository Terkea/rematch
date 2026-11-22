"use client";

import { dayText } from "@ci/ui/profile";
import { type ReactNode, useEffect, useRef } from "react";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import type { ChangeNoteView } from "../../../../server/coaching/notes";
import type { ReportView as ReportViewModel } from "../../../../server/coaching/report";
import { SECTION_KEYS, type SectionKey } from "../../../../server/coaching/types";
import type { CoachScope } from "../../../../server/profile/types";
import { Areas } from "./_sections/areas";
import { Changes } from "./_sections/changes";
import { ClimbPlan } from "./_sections/climb-plan";
import { Openings } from "./_sections/openings";
import { Overview } from "./_sections/overview";
import { GameIdsContext, type SectionStatus, sectionStatus } from "./_sections/shell";
import { Snapshot } from "./_sections/snapshot";
import { StateBanner } from "./_sections/state-banner";
import { Weaknesses } from "./_sections/weaknesses";
import { RebuildButton } from "./rebuild-button";
import { markSeen, revealScript, seenId } from "./reveal";

export type ReportPageData = {
  scope: CoachScope;
  view: ReportViewModel;
  /** The account's weakness change notes (Task 2 `changeNotes`). */
  notes: ChangeNoteView[];
  /** Weekly rates per weakness series, oldest first (null is a week with no chances). */
  trends: Record<string, (number | null)[]>;
  /** Platform game id to `games.id`, for the scope's games the report cites (`reviewGameIds`). */
  gameIds: Record<string, string>;
  now: Date;
};

const SHEET_ID = "coaching-report";

/** "2 statements were removed because they couldn't be checked against your games". */
export function droppedText(count: number): string | null {
  if (count <= 0) return null;
  return count === 1
    ? "1 statement was removed because it couldn't be checked against your games."
    : `${count} statements were removed because they couldn't be checked against your games.`;
}

/**
 * Ends the reveal on any key or pointer press (M7) and records the version as seen on this
 * device. Sections that become ready while the page is open assemble in on their own.
 */
function useReveal(id: string | null) {
  useEffect(() => {
    if (!id) return;
    const el = document.getElementById(SHEET_ID);
    let storage: Storage | null = null;
    try {
      storage = window.localStorage;
    } catch {
      storage = null;
    }
    markSeen(storage, id);
    const end = () => {
      el?.querySelectorAll("[data-reveal='first']").forEach((n) => {
        n.setAttribute("data-reveal", "done");
      });
      if (el?.getAttribute("data-reveal") === "first") el.setAttribute("data-reveal", "done");
    };
    window.addEventListener("keydown", end, { once: true });
    window.addEventListener("pointerdown", end, { once: true });
    return () => {
      window.removeEventListener("keydown", end);
      window.removeEventListener("pointerdown", end);
    };
  }, [id]);
}

/** Section keys that turned ready since the page first rendered, so they assemble in. */
function useFreshSections(ready: SectionKey[]): Set<SectionKey> {
  const initial = useRef<Set<SectionKey> | null>(null);
  if (initial.current === null) initial.current = new Set(ready);
  const known = initial.current;
  return new Set(ready.filter((k) => !known.has(k)));
}

function Fresh({ on, children }: { on: boolean; children: ReactNode }) {
  return on ? <div data-reveal="first">{children}</div> : <>{children}</>;
}

/**
 * The coaching report (ADR 0073), top to bottom: the state banner, where you stand (with the AI
 * disclosure), the coach's overview, what's costing you games, openings, the climb plan, the
 * area notes, this month's changes and the count of removed statements. Every section keeps its
 * computed part; the coach's part shows only when its row is `ready`.
 */
export function ReportView({ data }: { data: ReportPageData }) {
  const { scope, view, notes, trends, gameIds } = data;
  const { state, payload } = view;
  const building = state.kind === "building";
  const target = { accountId: scope.accountId, timeClass: scope.timeClass };
  const first =
    payload === null || (building && payload.version === 1 && payload.status !== "complete");
  const revealId = payload ? seenId(payload.reportId, payload.version) : null;
  useReveal(revealId);

  const readyKeys = payload ? SECTION_KEYS.filter((k) => Boolean(payload.sections[k])) : [];
  const fresh = useFreshSections(readyKeys);

  const header = (
    <header className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Your coaching report</h1>
          {view.createdAt ? (
            <p className="m-0 text-sm text-ink-soft">
              Report of{" "}
              <time dateTime={view.createdAt.toISOString().slice(0, 10)}>
                {dayText(view.createdAt.toISOString())}
              </time>
              , version {payload?.version ?? 1}
            </p>
          ) : null}
        </div>
        {payload ? (
          <RebuildButton
            scope={target}
            nextManualAt={view.nextManualAt}
            building={building}
            first={first}
          />
        ) : null}
      </div>
      {/* The switch's labels can be wider than a phone; it scrolls inside its own row. */}
      <div className="-mx-1 max-w-full overflow-x-auto px-1 py-1">
        <CoachScopeSwitch scope={scope} basePath="/app/report" />
      </div>
    </header>
  );

  const banner = (
    <StateBanner state={state} scope={target} first={first} nextManualAt={view.nextManualAt} />
  );

  if (!payload) {
    return (
      <div className="flex flex-col gap-10">
        {header}
        {banner}
      </div>
    );
  }

  const aiOff =
    payload.status === "computed_only" ||
    (state.kind === "ready" && state.aiOff !== null) ||
    (state.kind === "failed" && state.reason === "budget");
  const status = (k: SectionKey): SectionStatus =>
    sectionStatus(payload.sections[k], { building, aiOff });
  const content = <K extends keyof NonNullable<(typeof payload.sections)[SectionKey]>>(
    k: SectionKey,
    part: K,
  ) => payload.sections[k]?.[part] ?? null;
  const disclosure = readyKeys.length > 0 ? payload.aiDisclosure : null;
  const { computed } = payload;
  const dropped = droppedText(payload.dropped.filter((d) => d.path !== "").length);

  return (
    <GameIdsContext.Provider value={gameIds}>
      <div
        id={SHEET_ID}
        suppressHydrationWarning
        data-report-version={revealId ?? undefined}
        className="flex flex-col gap-12"
      >
        {revealId ? (
          // biome-ignore lint/security/noDangerouslySetInnerHtml: a fixed script with JSON-encoded ids; it sets the reveal before paint
          <script dangerouslySetInnerHTML={{ __html: revealScript(SHEET_ID, revealId) }} />
        ) : null}
        {header}
        {banner}
        <Snapshot computed={computed} disclosure={disclosure} order={0} />
        <Fresh on={fresh.has("overview")}>
          <Overview
            computed={computed}
            content={content("overview", "overview")}
            status={status("overview")}
            disclosure={disclosure}
            order={1}
          />
        </Fresh>
        <Fresh on={fresh.has("weaknesses")}>
          <Weaknesses
            computed={computed}
            content={content("weaknesses", "weaknesses")}
            status={status("weaknesses")}
            disclosure={disclosure}
            scope={target}
            trends={trends}
            reportDate={(view.createdAt ?? new Date(payload.generatedAt)).toISOString()}
            order={2}
          />
        </Fresh>
        <Fresh on={fresh.has("openings")}>
          <Openings
            computed={computed}
            content={content("openings", "openings")}
            status={status("openings")}
            disclosure={disclosure}
            order={3}
          />
        </Fresh>
        <Fresh on={fresh.has("climb_plan")}>
          <ClimbPlan
            computed={computed}
            content={content("climb_plan", "climbPlan")}
            status={status("climb_plan")}
            disclosure={disclosure}
            order={4}
          />
        </Fresh>
        <Fresh on={fresh.has("areas")}>
          <Areas
            content={content("areas", "areas")}
            status={status("areas")}
            disclosure={disclosure}
            scope={target}
            order={5}
          />
        </Fresh>
        <Changes computed={computed} notes={notes} order={6} />
        {dropped ? <p className="m-0 text-sm text-ink-soft">{dropped}</p> : null}
      </div>
    </GameIdsContext.Provider>
  );
}
