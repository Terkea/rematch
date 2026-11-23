import { RoundPage } from "../../_lib/page-guard";
import { WhyInline } from "../../_lib/why";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="why"
      variant="a"
      title="A. Opens in place"
      rationale="The focus card grows a ruled list of facts, the ranking rules and the version line, in place."
    >
      <WhyInline />
    </RoundPage>
  );
}
