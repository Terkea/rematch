"use client";

import { formatNotation, moveNumberText } from "@ci/ui/board";
import type { AiDisclosureData } from "@ci/ui/coach";
import { ExplainPopover } from "@ci/ui/profile";
import { type LineEvidence, lineEvidenceKey, verdictWhy } from "../../../../../lib/explain";
import type {
  ComputedReport,
  OpeningsSection,
  OpeningVerdict,
  Verdict,
} from "../../../../../server/coaching/types";
import { VERDICT_GLOSSARY } from "../../../../../server/profile/glossary";
import { CoachText, ReportSection, SectionNote, type SectionStatus } from "./shell";

export const VERDICT_TEXT: Record<Verdict, string> = {
  keep: "Keep",
  fix: "Fix",
  learn: "Learn deeper",
  replace: "Replace",
};

const pct = (v: number) => `${Math.round(v * 100)}%`;

/** "1. e4 e5 2. Nf3 Nc6 3. Bc4": White's moves numbered, Black's following. */
export function lineText(moves: string[]): string {
  return moves
    .map((san, i) => (i % 2 === 0 ? `${moveNumberText(i / 2 + 1, "white")} ${san}` : san))
    .join(" ");
}

function resultText(o: OpeningVerdict): string {
  const games = `${o.games} ${o.games === 1 ? "game" : "games"}`;
  const score = Number.isFinite(o.score) ? `, scoring ${pct(o.score)}` : "";
  const perf = o.performanceVsExpected;
  let vs = "";
  if (perf !== null && Number.isFinite(perf)) {
    const points = Math.round(Math.abs(perf) * 100);
    vs =
      points === 0
        ? ", as your rating predicts"
        : `, ${points} percentage ${points === 1 ? "point" : "points"} ${perf < 0 ? "below" : "above"} what your rating predicts`;
  }
  return `${games}${score}${vs}`;
}

function deviationText(o: OpeningVerdict): string | null {
  if (o.deviationPly === null || !o.deviationMoveSan || o.deviationPly < 1) return null;
  const color = o.deviationPly % 2 === 1 ? "white" : "black";
  const move = formatNotation({
    moveNumber: Math.ceil(o.deviationPly / 2),
    color,
    san: o.deviationMoveSan,
  } as never);
  return `Your games leave the main line at ${move}.`;
}

/**
 * Why the line has its verdict: from the stored rule evidence when it still gives the verdict the
 * report shows, otherwise the verdict's glossary meaning (never evidence for another verdict).
 */
export function verdictReason(o: OpeningVerdict, lines: Record<string, LineEvidence>): string {
  const e = lines[lineEvidenceKey(o.lineKey, o.color, o.side)];
  if (e && e.verdict === o.verdict) return verdictWhy(e);
  return VERDICT_GLOSSARY[o.verdict]?.meaning ?? "";
}

/** The openings with their engine verdict (Keep, Fix, Learn deeper, Replace) and the coach's note. */
export function Openings({
  computed,
  content,
  status,
  disclosure,
  lines = {},
  order,
}: {
  computed: ComputedReport;
  content: OpeningsSection | null;
  status: SectionStatus;
  disclosure: AiDisclosureData | null;
  /** Stored verdict evidence by `lineEvidenceKey` (I9). */
  lines?: Record<string, LineEvidence>;
  order: number;
}) {
  const ready = status === "ready" && content !== null;
  const items = ready ? content.items : [];
  return (
    <ReportSection
      id="report-openings"
      title="Openings"
      order={order}
      disclosure={ready && items.length > 0 ? disclosure : null}
    >
      {ready ? null : <SectionNote status={status} />}
      {computed.openings.length === 0 ? (
        <p className="m-0 text-sm text-ink-soft">No opening has enough games for a verdict yet.</p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-6 p-0">
          {computed.openings.map((o) => {
            const note = items.find((it) => it.lineKey === o.lineKey);
            const deviation = deviationText(o);
            const entry = VERDICT_GLOSSARY[o.verdict];
            const reason = verdictReason(o, lines);
            return (
              <li key={o.lineKey} className="flex flex-col gap-1.5">
                <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                  <h3 className="m-0 text-base font-medium text-ink">{o.name}</h3>
                  <span className="inline-flex items-center gap-1 text-sm text-ink">
                    <span>
                      <span className="text-ink-soft">Verdict: </span>
                      <span className="font-medium">{VERDICT_TEXT[o.verdict] ?? o.verdict}</span>
                    </span>
                    {entry ? (
                      <ExplainPopover
                        entry={entry}
                        label={`What does ${VERDICT_TEXT[o.verdict] ?? entry.name} mean?`}
                      />
                    ) : null}
                  </span>
                </div>
                <p className="m-0 text-sm text-ink-soft">
                  {o.side === "chosen" ? "You play it" : "You face it"} as{" "}
                  {o.color === "white" ? "White" : "Black"}: {resultText(o)}.
                </p>
                {reason ? (
                  <p
                    className="m-0 max-w-prose text-base text-ink"
                    style={{ fontVariantNumeric: "lining-nums" }}
                  >
                    {reason}
                  </p>
                ) : null}
                <p className="m-0 font-serif text-sm text-ink-soft">{lineText(o.movesSan)}</p>
                {deviation ? <p className="m-0 text-sm text-ink-soft">{deviation}</p> : null}
                {note ? <CoachText item={note} className="mt-1 text-base" /> : null}
              </li>
            );
          })}
        </ul>
      )}
    </ReportSection>
  );
}
