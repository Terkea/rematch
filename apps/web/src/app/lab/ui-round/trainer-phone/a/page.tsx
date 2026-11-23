import { RoundPage } from "../../_lib/page-guard";
import { PhoneA } from "../../_lib/phone-trainer";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="trainer-phone"
      variant="a"
      title="A. Board in the gutter, answers below"
      rationale="Today's structure, tightened: the app header, the board at the full gutter width (352 px), the question and the three answers under it."
    >
      <PhoneA />
    </RoundPage>
  );
}
