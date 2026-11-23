import { RoundPage } from "../../_lib/page-guard";
import { WhyPopover } from "../../_lib/why";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="why"
      variant="c"
      title="C. A short popover"
      rationale="Three facts and the version in a popover on the link, with The full reasoning for the rest."
    >
      <WhyPopover />
    </RoundPage>
  );
}
