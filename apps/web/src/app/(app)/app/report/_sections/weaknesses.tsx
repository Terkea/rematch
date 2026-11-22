"use client";

import type { AiDisclosureData } from "@ci/ui/coach";
import { dayText, ExemplarBoard, WeaknessRow, weaknessScaleMax } from "@ci/ui/profile";
import type { CSSProperties } from "react";
import { coachHref } from "../../../../../server/coach/anchor";
import type { ComputedReport, WeaknessesSection } from "../../../../../server/coaching/types";
import type { ReportScope } from "../rebuild-button";
import { weaknessHref, weaknessSeriesKey } from "./links";
import { CoachText, ReportSection, SectionNote, type SectionStatus } from "./shell";
import { BAND_TEXT } from "./snapshot";

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/**
 * "What's costing you games": the ranked weaknesses, each with its row (stored estimate, interval,
 * counts), the coach's note for that weakness, up to three positions from the report and a link
 * to ask the coach about it. In the reveal the rows resolve in rank order.
 */
export function Weaknesses({
  computed,
  content,
  status,
  disclosure,
  scope,
  trends,
  reportDate,
  order,
}: {
  computed: ComputedReport;
  content: WeaknessesSection | null;
  status: SectionStatus;
  disclosure: AiDisclosureData | null;
  scope: ReportScope;
  trends: Record<string, (number | null)[]>;
  reportDate: string;
  order: number;
}) {
  const ready = status === "ready" && content !== null;
  const ranked = [...computed.weaknesses].sort((a, b) => a.rank - b.rank);
  const items = ready ? content.items : [];
  const scaleMax = weaknessScaleMax(
    ranked.map((w) => ({
      rate: { mean: w.posteriorRate, low: w.intervalLow, high: w.intervalHigh },
      band: { median: w.bandMedian },
    })),
  );
  const band = capital(BAND_TEXT[computed.player.band] ?? "players at your level");
  return (
    <ReportSection
      id="report-weaknesses"
      title="What's costing you games"
      order={order}
      disclosure={ready ? disclosure : null}
      intro={
        <p className="m-0 text-sm text-ink-soft">
          Ranked by how much each one costs you, compared with {band.toLowerCase()}.
        </p>
      }
    >
      {ready ? null : <SectionNote status={status} />}
      {ranked.length === 0 ? (
        <p className="m-0 text-sm text-ink-soft">
          Nothing stands out above {band.toLowerCase()} in this report.
        </p>
      ) : (
        <ol className="m-0 flex list-none flex-col gap-10 p-0">
          {ranked.map((w, i) => {
            const seriesKey = weaknessSeriesKey(w);
            const note = items.find((it) => it.weaknessId === w.id);
            const boards = w.exemplars.slice(0, 3);
            return (
              <li
                key={seriesKey}
                data-reveal-fig=""
                style={{ "--reveal-i": order + 1 + i } as CSSProperties}
                className="flex flex-col gap-4"
              >
                <div className="flex items-start gap-3">
                  <span
                    aria-hidden="true"
                    className="w-6 shrink-0 pt-3.5 text-right font-serif text-xl font-semibold text-ink-faint"
                    style={{ fontVariantNumeric: "lining-nums tabular-nums" }}
                  >
                    {w.rank}
                  </span>
                  <WeaknessRow
                    className="min-w-0 flex-1"
                    label={w.label}
                    state={w.state}
                    rate={{ mean: w.posteriorRate, low: w.intervalLow, high: w.intervalHigh }}
                    evidence={{
                      events: w.events,
                      chances: w.opportunities,
                      sufficient: true,
                      needMore: null,
                    }}
                    band={{
                      median: Number.isFinite(w.bandMedian) ? w.bandMedian : null,
                      label: band,
                    }}
                    href={weaknessHref(seriesKey, scope)}
                    trend={trends[seriesKey] ?? []}
                    onset={null}
                    scaleMax={scaleMax}
                  />
                </div>
                <div className="flex flex-col gap-4 pl-9">
                  {note ? <CoachText item={note} /> : null}
                  {boards.length > 0 ? (
                    <figure className="m-0 flex flex-col gap-3">
                      <figcaption className="text-sm text-ink-soft">
                        Examples from your report of{" "}
                        <time dateTime={reportDate.slice(0, 10)}>{dayText(reportDate)}</time>
                      </figcaption>
                      {/* ban-lint-allow TOK2: three report boards side by side fit 20rem each, rounded to whole pixels per square */}
                      <div className="grid grid-cols-1 gap-6 [--board-size:round(down,min(100vw_-_2*var(--space-gutter)_-_2.25rem,20rem),8px)] lg:grid-cols-3">
                        {boards.map((e) => (
                          <ExemplarBoard
                            key={`${e.gameId}:${e.ply}`}
                            fen={e.fen}
                            played={e.played}
                            best={e.best}
                            refutation={e.refutationPv[0] ?? null}
                            caption={{ winBefore: e.winBefore, winAfter: e.winAfter }}
                          />
                        ))}
                      </div>
                    </figure>
                  ) : null}
                  <a
                    href={coachHref({ kind: "weakness", weaknessId: w.id }, scope)}
                    className="self-start rounded-field text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
                  >
                    Ask your coach about {w.label.toLowerCase()}
                  </a>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </ReportSection>
  );
}
