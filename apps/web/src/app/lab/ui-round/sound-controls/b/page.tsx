import { BoardSoundBar } from "../../_lib/board-sound-bar";
import { RoundPage } from "../../_lib/page-guard";
import { MockHeader } from "../../_lib/shell";
import { TrainerScene } from "../../_lib/trainer-scene";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="sound-controls"
      variant="b"
      title="B. On the board's own toolbar"
      rationale="Sound lives where it is heard: a toolbar under every board (flip, sound). The header stays as it is. Volume and the sound set are in Settings."
    >
      <MockHeader sound="none" />
      <TrainerScene belowBoard={<BoardSoundBar open />} />
    </RoundPage>
  );
}
