"use client";

import { areaLabels } from "@ci/ui/area-score";
import type { AiDisclosureData } from "@ci/ui/coach";
import type { ComputedReport, OverviewSection } from "../../../../../server/coaching/types";
import type { ReportScope } from "../rebuild-button";
import { strengthLabel, strengthsHref } from "./links";
import { CoachText, ReportSection, SectionNote, type SectionStatus } from "./shell";

/** The coach's headline, summary and strengths over the strengths the engine found. */
export function Overview({
  computed,
  content,
  status,
  disclosure,
  scope,
  order,
}: {
  computed: ComputedReport;
  /** The report's scope, for the link to its Strengths page. */
  scope?: ReportScope;
  content: OverviewSection | null;
  status: SectionStatus;
  disclosure: AiDisclosureData | null;
  order: number;
}) {
  const ready = status === "ready" && content !== null;
  return (
    <ReportSection
      id="report-overview"
      title="Overview"
      order={order}
      disclosure={ready ? disclosure : null}
    >
      {ready ? (
        <div className="flex flex-col gap-3">
          <CoachText item={content.headline} className="text-2xl leading-snug font-semibold" />
          <CoachText item={content.summary} />
          {content.strengths.length > 0 ? (
            <ul className="m-0 flex list-none flex-col gap-3 p-0">
              {content.strengths.map((s, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: a fixed, ordered list of statements
                <li key={i} className="flex flex-col gap-1">
                  <span className="text-sm font-medium text-good-text">
                    Strength: {areaLabels[s.area] ?? s.area}
                  </span>
                  <CoachText item={s} />
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : (
        <SectionNote status={status} />
      )}
      {computed.strengths.length > 0 ? (
        <div className="flex flex-col gap-2">
          <h3 className="m-0 text-base font-medium text-ink">Strengths found in your games</h3>
          <ul className="m-0 flex list-none flex-col gap-1 p-0 text-sm text-ink-soft">
            {computed.strengths.map((s) => (
              <li key={`${s.area}:${s.label}`}>
                <span className="text-ink">{strengthLabel(s)}</span>{" "}
                ({areaLabels[s.area] ?? s.area})
              </li>
            ))}
          </ul>
          {scope ? (
            <a
              href={strengthsHref(scope)}
              className="inline-flex min-h-6 items-center self-start rounded-field text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
            >
              See everything you do well
            </a>
          ) : null}
        </div>
      ) : null}
    </ReportSection>
  );
}
