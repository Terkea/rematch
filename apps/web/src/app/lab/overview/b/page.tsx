import { notFound } from "next/navigation";
import { labEnabled } from "../../lab-enabled";
import { LabFrame } from "../../lab-frame";
import { loadLabOverview } from "../load";
import { toViewB } from "../view";
import { OverviewB } from "./view";

export const dynamic = "force-dynamic";

// Lab only, on the signed-in user's real games. Guarded here too, before any data is read.
export default async function OverviewLabB() {
  if (!labEnabled(process.env)) notFound();
  const overview = await loadLabOverview();
  return (
    <LabFrame
      surface="overview"
      variant="b"
      title="Chart-led"
      rationale="The accuracy trend runs full width as the opening statement, with the headline numbers written as one sentence above it; from lg the KPI rows sit beside the judgement columns and results."
    >
      {/* Mapped on the server: the client view gets only what it renders. */}
      <OverviewB view={toViewB(overview)} />
    </LabFrame>
  );
}
