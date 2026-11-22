"use client";

import { CoachState } from "@ci/ui/coach";
import type { ReportState } from "../../../../../server/coaching/state";
import { SECTION_KEYS, type SectionKey } from "../../../../../server/coaching/types";
import { type ReportScope, useRequestReport, utcTime, whenText } from "../rebuild-button";

/** The five sections by name, in `SECTION_KEYS` order (the progress list matches "N of 5"). */
export const SECTION_NAMES: Record<SectionKey, string> = {
  overview: "Overview",
  weaknesses: "What's costing you games",
  openings: "Openings",
  climb_plan: "Your climb plan",
  areas: "Area notes",
};

export const BUDGET_COPY =
  "Your coach has reached this month's spending limit, so this report shows the computed numbers only.";

/**
 * The report's state, in words (ADR 0073). `first` says the scope has no report before this one
 * (ruling G3: the first report starts on its own once the profile is built); `nextManualAt` is
 * when a manual rebuild becomes possible.
 */
export function StateBanner({
  state,
  scope,
  first,
  nextManualAt,
  now,
  notesCount,
}: {
  state: ReportState;
  scope: ReportScope;
  first: boolean;
  nextManualAt: Date | null;
  /** The render's clock (the server's), so server and client agree on "at" or "on". */
  now: Date;
  /** How many sections of the shown report have coach notes. */
  notesCount?: number;
}) {
  const { request, pending } = useRequestReport(scope, first);
  const retry = {
    label: pending ? "Requesting your report" : "Try again",
    onClick: () => void request(),
  };

  switch (state.kind) {
    case "none":
      return (
        <CoachState
          kind="empty"
          titleAs="h2"
          title="Your coaching report hasn't been built yet."
          body="It's written from your analysed games, and its sections appear as your coach writes them."
          action={{
            label: pending ? "Requesting your report" : "Build my report",
            onClick: () => void request(),
          }}
        />
      );
    case "building": {
      const ready = new Set(state.ready);
      const done = SECTION_KEYS.filter((k) => ready.has(k)).length;
      const lines = [
        // Not "started on its own": a first run from G3 and one from "Build my report" look the
        // same on the run row (both `onboarding`, `is_first_report`), so the copy claims neither.
        first ? "Your first report is being written." : null,
        state.batch ? "Waiting for the batch, usually within an hour." : null,
        state.retryAt ? `Your coach is busy; retrying at ${utcTime(state.retryAt)}.` : null,
      ].filter((l): l is string => l !== null);
      return (
        <CoachState
          kind="building"
          titleAs="h2"
          title={`Building your report: ${done} of ${SECTION_KEYS.length} sections written`}
          body={lines.length > 0 ? lines.join(" ") : null}
          items={SECTION_KEYS.map((k) => ({ label: SECTION_NAMES[k], done: ready.has(k) }))}
        />
      );
    }
    case "ready":
      if (state.aiOff === "budget") {
        return <CoachState kind="budget" titleAs="h2" title={BUDGET_COPY} />;
      }
      if (state.aiOff === "no_api_key") {
        return (
          <CoachState
            kind="computed_only"
            titleAs="h2"
            title="Your coach's notes are off for now. The numbers below are computed from your games."
          />
        );
      }
      if (state.status === "partial" && notesCount !== undefined) {
        return (
          <CoachState
            kind="computed_only"
            titleAs="h2"
            title={`${notesCount} of ${SECTION_KEYS.length} sections have your coach's notes. The others show the computed numbers.`}
          />
        );
      }
      if (state.status === "computed_only") {
        return (
          <CoachState
            kind="computed_only"
            titleAs="h2"
            title="This report has no coach's notes. The numbers below are computed from your games."
          />
        );
      }
      return null;
    case "failed":
      switch (state.reason) {
        case "plan_interval":
          return (
            <CoachState
              kind="unavailable"
              titleAs="h2"
              title={
                nextManualAt
                  ? `You can rebuild this report ${whenText(nextManualAt, now)}.`
                  : "The last rebuild came too soon after the previous report. You can rebuild it now."
              }
            />
          );
        case "no_input":
          return (
            <CoachState
              kind="unavailable"
              titleAs="h2"
              title="There isn't enough analysed data for a report yet."
              body="Your report is written once enough of your games are analysed. Analysis continues in the background."
            />
          );
        case "budget":
          return <CoachState kind="budget" titleAs="h2" title={BUDGET_COPY} />;
        default:
          return state.hasPrevious ? (
            <CoachState
              kind="failed"
              titleAs="h2"
              title="The last rebuild failed. Your previous report is still shown."
            />
          ) : (
            <CoachState
              kind="failed"
              titleAs="h2"
              title="Building your report failed. Try again."
              action={retry}
            />
          );
      }
  }
}
