"use client";

import { AiDisclosure } from "@ci/ui/coach";
import { dayText } from "@ci/ui/profile";
import { stateStyle } from "@ci/ui/timeline";
import type { ChangeNoteView } from "../../../../../server/coaching/notes";
import type { ComputedReport, TimelineChange } from "../../../../../server/coaching/types";
import { CoachText, ReportSection } from "./shell";

const pct = (v: number) => (Number.isFinite(v) ? `${Math.round(v * 1000) / 10}%` : null);
const stateLabel = (s: string) =>
  (stateStyle as Record<string, { label: string } | undefined>)[s]?.label.toLowerCase() ?? s;

function DayTime({ iso }: { iso: string }) {
  return <time dateTime={iso.slice(0, 10)}>{dayText(iso)}</time>;
}

function Change({ change, note }: { change: TimelineChange; note: ChangeNoteView | undefined }) {
  const before = pct(change.rateBefore);
  const after = pct(change.rateAfter);
  const causes = change.candidateCauses.filter((c) => c.description.trim().length > 0);
  return (
    <li className="flex flex-col gap-1.5">
      <h4 className="m-0 text-base font-medium text-ink">{change.label}</h4>
      <p className="m-0 text-sm text-ink-soft">
        Detected as {stateLabel(change.toState)} (was {stateLabel(change.fromState)}) from about{" "}
        <DayTime iso={change.onsetDate} />, somewhere between <DayTime iso={change.onsetFrom} /> and{" "}
        <DayTime iso={change.onsetTo} />.
        {before && after ? ` Estimated rate ${before} before, ${after} after.` : null}
      </p>
      {causes.length > 0 ? (
        <p className="m-0 text-sm text-ink-soft">
          Possible causes: {causes.map((c) => c.description).join("; ")}.
        </p>
      ) : null}
      {note?.item ? (
        <div className="mt-1 flex flex-col gap-2">
          <CoachText item={note.item} className="text-base" />
          {note.aiDisclosure ? <AiDisclosure disclosure={note.aiDisclosure} compact /> : null}
        </div>
      ) : null}
    </li>
  );
}

function ChangeList({
  id,
  title,
  changes,
  empty,
  notes,
}: {
  id: string;
  title: string;
  changes: TimelineChange[];
  empty: string;
  notes: ChangeNoteView[];
}) {
  return (
    <div className="flex flex-col gap-3">
      <h3 id={id} className="m-0 font-serif text-lg font-semibold text-ink">
        {title}
      </h3>
      {changes.length === 0 ? (
        <p className="m-0 text-sm text-ink-soft">{empty}</p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-5 p-0">
          {changes.map((c) => (
            <Change
              key={`${c.seriesKey}:${c.onsetDate}`}
              change={c}
              note={notes.find(
                (n) => n.seriesKey === c.seriesKey && n.status === "ready" && n.item !== null,
              )}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

/** "New this month" and "Fixed this month": the detected changes, with their ready change notes. */
export function Changes({
  computed,
  notes,
  order,
}: {
  computed: ComputedReport;
  notes: ChangeNoteView[];
  order: number;
}) {
  return (
    <ReportSection id="report-changes" title="This month" order={order}>
      <div className="grid grid-cols-1 gap-x-12 gap-y-8 lg:grid-cols-2">
        <ChangeList
          id="report-new"
          title="New this month"
          changes={computed.newThisPeriod}
          empty="No new problem was detected this month."
          notes={notes}
        />
        <ChangeList
          id="report-fixed"
          title="Fixed this month"
          changes={computed.fixedThisPeriod}
          empty="Nothing was detected as fixed this month."
          notes={notes}
        />
      </div>
    </ReportSection>
  );
}
