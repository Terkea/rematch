import { RoundPage } from "../../_lib/page-guard";
import { SettingsSideNav } from "../../_lib/settings";
import { MockHeader } from "../../_lib/shell";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="settings"
      variant="b"
      title="B. Sections beside the content"
      rationale="A section list on the left (a row on phones) and one section at a time, so the board previews get the width."
    >
      <MockHeader current="" sound="none" />
      <SettingsSideNav />
    </RoundPage>
  );
}
