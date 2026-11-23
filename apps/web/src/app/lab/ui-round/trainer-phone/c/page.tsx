import { RoundPage } from "../../_lib/page-guard";
import { PhoneC } from "../../_lib/phone-trainer";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="trainer-phone"
      variant="c"
      title="C. Focus mode"
      rationale="Training hides the app nav: a Stop for today button, the strip and the count, then the board and three full-width answers in the thumb zone."
    >
      <PhoneC />
    </RoundPage>
  );
}
