import { RoundPage } from "../../_lib/page-guard";
import { PhoneB } from "../../_lib/phone-trainer";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="trainer-phone"
      variant="b"
      title="B. Edge to edge, a thumb bar"
      rationale="The board takes the whole phone width (384 px, 48 px squares), the session strip runs under it, and the answers sit in a bar pinned to the bottom."
    >
      <PhoneB />
    </RoundPage>
  );
}
