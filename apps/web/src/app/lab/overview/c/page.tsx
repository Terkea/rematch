import { notFound } from "next/navigation";
import { labEnabled } from "../../lab-enabled";
import { LabFrame } from "../../lab-frame";
import { loadLabOverviewByClass } from "../load";
import { toViewC } from "../view";
import { OverviewC } from "./view";

export const dynamic = "force-dynamic";

// Lab only, on the signed-in user's real games. Guarded here too, before any data is read.
export default async function OverviewLabC() {
  if (!labEnabled(process.env)) notFound();
  const { all, classes } = await loadLabOverviewByClass();
  return (
    <LabFrame
      surface="overview"
      variant="c"
      title="By time control first"
      rationale="The time controls are the spine: each row states its games, results and score, and opens to that time control's own accuracy trend and judgements. The largest time control opens first."
    >
      {/* Mapped on the server: the client view gets only what it renders. */}
      <OverviewC view={toViewC(all, classes)} />
    </LabFrame>
  );
}
