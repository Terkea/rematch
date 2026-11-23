import { OverviewC } from "../../_lib/overview";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="overview"
      variant="c"
      title="C. Today first"
      rationale="The page opens on today's practice as its headline; focus and trajectory are two short sheets; every other number moves to its own page."
    >
      <OverviewC />
    </RoundPage>
  );
}
