import { notFound } from "next/navigation";
import { labEnabled } from "../../lab-enabled";
import { LabFrame } from "../../lab-frame";
import { loadLabOverview } from "../load";
import { OverviewA } from "./view";

export const dynamic = "force-dynamic";

// Lab only, on the signed-in user's real games. Guarded here too, before any data is read.
export default async function OverviewLabA() {
  if (!labEnabled(process.env)) notFound();
  const overview = await loadLabOverview();
  return (
    <LabFrame
      surface="overview"
      variant="a"
      title="Scoresheet column"
      rationale="One reading column ruled like a scoresheet: header fields for the account and games, then each KPI as a ruled row carrying its own chart, so a number and its evidence are never apart."
    >
      <OverviewA overview={overview} />
    </LabFrame>
  );
}
