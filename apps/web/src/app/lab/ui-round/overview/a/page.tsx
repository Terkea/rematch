import { OverviewA } from "../../_lib/overview";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="overview"
      variant="a"
      title="A. Three numbered steps, then More detail"
      rationale="Where you are, this week's focus and Practise now as three numbered sheets; everything else is a closed list of one-line summaries."
    >
      <OverviewA />
    </RoundPage>
  );
}
