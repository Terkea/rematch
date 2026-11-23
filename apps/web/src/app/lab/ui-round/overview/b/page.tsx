import { OverviewB } from "../../_lib/overview";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="overview"
      variant="b"
      title="B. The coach's note"
      rationale="One sentence says the trajectory, the leak and the plan, with the practise button under it and the rating chart beside it; the numbers sit behind tabs."
    >
      <OverviewB />
    </RoundPage>
  );
}
