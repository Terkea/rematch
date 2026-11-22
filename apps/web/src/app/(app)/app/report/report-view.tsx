"use client";

import { AiDisclosure } from "@ci/ui/coach";
import { dayText } from "@ci/ui/profile";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import type { LineEvidence } from "../../../../lib/explain";
import type { CoachStatus } from "../../../../server/coach/status";
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
  /** The scope's stored opening lines' verdict evidence, by `lineEvidenceKey` (I9). */
  lines?: Record<string, LineEvidence>;
  /** Whether a coach could write a report now (`ai_status`, ADR 0078). Default available. */
  coach?: CoachStatus;
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

/**
 * The fade over the scrolling options themselves, only on an edge with more past it, so a cut
 * label always reads as "more this way". A mask is alpha only, not a colour.
 */
const FADE = {
  none: "",
  end: "linear-gradient(to right, black calc(100% - 3rem), transparent)",
  start: "linear-gradient(to left, black calc(100% - 3rem), transparent)",
  both: "linear-gradient(to right, transparent, black 3rem, black calc(100% - 3rem), transparent)",
} as const;

/**
 * The scope switch's row. Its options can be wider than a phone and scroll inside the switch's
 * own scroller: the selected option is scrolled into view, and each edge of that scroller with
 * more options past it fades.
 */
function ScopeRow({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [fade, setFade] = useState<keyof typeof FADE>("none");
  useEffect(() => {
    const root = ref.current;
    const scroller = root
      ? [...root.querySelectorAll<HTMLElement>("*")].find((n) =>
          ["auto", "scroll"].includes(getComputedStyle(n).overflowX),
        )
      : undefined;
    if (!scroller) return;
    const selected = scroller.querySelector<HTMLElement>("[data-checked]");
    if (selected) {
      const s = selected.getBoundingClientRect();
      const c = scroller.getBoundingClientRect();
      if (s.right > c.right) scroller.scrollLeft += s.right - c.right + 48;
      else if (s.left < c.left) scroller.scrollLeft -= c.left - s.left + 48;
    }
    const update = () => {
      const start = scroller.scrollLeft > 1;
      const end = scroller.scrollLeft + scroller.clientWidth < scroller.scrollWidth - 1;
      const next = start && end ? "both" : end ? "end" : start ? "start" : "none";
      scroller.style.maskImage = FADE[next];
      scroller.style.webkitMaskImage = FADE[next];
      setFade(next);
    };
    update();
    scroller.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      scroller.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, []);
  return (
    <div ref={ref} data-fade={fade} className="min-w-0">
      {children}
    </div>
  );
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
  const { scope, view, notes, trends, gameIds, lines } = data;
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
            now={data.now}
          />
        ) : null}
      </div>
      <ScopeRow>
        <CoachScopeSwitch scope={scope} basePath="/app/report" />
      </ScopeRow>
    </header>
  );

  const banner = (
    <StateBanner
      state={state}
      scope={target}
      first={first}
      nextManualAt={view.nextManualAt}
      now={data.now}
      notesCount={readyKeys.length}
      coach={data.coach ?? "available"}
    />
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
        {dropped ? <p className="m-0 -mt-6 text-sm text-ink-soft">{dropped}</p> : null}
        <Snapshot computed={computed} order={0} />
        {disclosure ? <AiDisclosure disclosure={disclosure} /> : null}
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
            lines={lines ?? {}}
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
            scores={computed.areaScores}
            status={status("areas")}
            disclosure={disclosure}
            scope={target}
            order={5}
          />
        </Fresh>
        <Changes computed={computed} notes={notes} order={6} />
      </div>
    </GameIdsContext.Provider>
  );
}
