import { AccountMenuPanel } from "../../_lib/account-menu";
import { RoundPage } from "../../_lib/page-guard";
import { MockHeader } from "../../_lib/shell";
import { TrainerScene } from "../../_lib/trainer-scene";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="sound-controls"
      variant="c"
      title="C. In the account menu"
      rationale="No new header button: the account menu gains a Sound row (a switch and the volume) and a Settings link. The header stays as quiet as it is now."
    >
      <MockHeader sound="menu" open panel={<AccountMenuPanel />} />
      <TrainerScene />
    </RoundPage>
  );
}
