import { RoundPage } from "../../_lib/page-guard";
import { WhyPanel } from "../../_lib/why";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="why"
      variant="b"
      title="B. A side panel"
      rationale="A panel beside the page (a bottom sheet on phones) with the facts, the rules, the version and a link to the full ranking."
    >
      <WhyPanel />
    </RoundPage>
  );
}
