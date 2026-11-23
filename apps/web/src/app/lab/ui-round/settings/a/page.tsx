import { RoundPage } from "../../_lib/page-guard";
import { SettingsLong } from "../../_lib/settings";
import { MockHeader } from "../../_lib/shell";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="settings"
      variant="a"
      title="A. One long page"
      rationale="Sound, Board and Moves as three sheets on one page, reached from the header panel link All settings."
    >
      <MockHeader current="" sound="header" />
      <SettingsLong />
    </RoundPage>
  );
}
