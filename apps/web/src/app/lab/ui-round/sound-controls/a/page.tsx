import { RoundPage } from "../../_lib/page-guard";
import { FloatPanel, MockHeader, SoundPanelBody } from "../../_lib/shell";
import { TrainerScene } from "../../_lib/trainer-scene";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="sound-controls"
      variant="a"
      title="A. A speaker in the header"
      rationale="One speaker button beside the theme toggle, on every page. It opens a small panel: sound on or off, volume, the pen scratch and a link to Settings."
    >
      <MockHeader
        sound="header"
        open
        panel={
          <FloatPanel>
            <p className="m-0 text-base font-medium text-ink">Sound</p>
            <SoundPanelBody />
          </FloatPanel>
        }
      />
      <TrainerScene />
    </RoundPage>
  );
}
