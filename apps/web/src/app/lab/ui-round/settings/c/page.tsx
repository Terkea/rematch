import { RoundPage } from "../../_lib/page-guard";
import { SettingsTabs } from "../../_lib/settings";
import { MockHeader } from "../../_lib/shell";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="settings"
      variant="c"
      title="C. Tabs"
      rationale="The same three sections as tabs; the account menu link opens on the last one used."
    >
      <MockHeader current="" sound="menu" />
      <SettingsTabs />
    </RoundPage>
  );
}
