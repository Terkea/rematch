"use client";

import { areaLabels } from "@ci/ui/area-score";
import type { AiDisclosureData } from "@ci/ui/coach";
import type { AreaId, AreaScoreRow, AreasSection } from "../../../../../server/coaching/types";
import type { ReportScope } from "../rebuild-button";
import { areaHref } from "./links";
import { CoachText, ReportSection, SectionNote, type SectionStatus } from "./shell";
import { noScoreText } from "./snapshot";

const AREA_ORDER: AreaId[] = [
  "openings",
  "middlegame",
  "endgame",
  "tactics",
  "calculation",
  "time",
  "mindset",
];

/** The coach's note on each of the seven areas, each linked to its area page. */
export function Areas({
  content,
  scores,
  status,
  disclosure,
  scope,
  order,
}: {
  content: AreasSection | null;
  /** The computed area scores, so every area keeps a number when there is no note. */
  scores: AreaScoreRow[];
  status: SectionStatus;
  disclosure: AiDisclosureData | null;
  scope: ReportScope;
  order: number;
}) {
  const ready = status === "ready" && content !== null;
  const byArea = new Map((ready ? content.items : []).map((i) => [i.area, i]));
  return (
    <ReportSection
      id="report-areas"
      title="Area notes"
      order={order}
      disclosure={ready ? disclosure : null}
    >
      {ready ? null : <SectionNote status={status} />}
      <ul className="m-0 grid list-none grid-cols-1 gap-x-12 gap-y-5 p-0 lg:grid-cols-2">
        {AREA_ORDER.map((area) => {
          const item = byArea.get(area);
          const row = scores.find((r) => r.area === area);
          const score =
            area !== "mindset" &&
            row &&
            row.score !== null &&
            Number.isFinite(row.score) &&
            row.sufficient
              ? `Score ${Math.round(row.score)}, from ${row.n} games`
              : noScoreText(area, row?.needMore ?? null);
          return (
            <li key={area} className="flex flex-col gap-1">
              <a
                href={areaHref(area, scope)}
                className="inline-flex min-h-6 items-center self-start rounded-field font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
              >
                {areaLabels[area]}
              </a>
              <span className="text-sm text-ink-soft" style={{ fontVariantNumeric: "lining-nums" }}>
                {score}
              </span>
              {item ? (
                <CoachText item={item} className="text-base" />
              ) : ready ? (
                <span className="text-sm text-ink-faint">No note on this area in this report</span>
              ) : null}
            </li>
          );
        })}
      </ul>
    </ReportSection>
  );
}
