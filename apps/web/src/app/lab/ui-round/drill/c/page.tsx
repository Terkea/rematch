import { C_RIGHT, C_WRONG, Sequence } from "../../_lib/drill";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="drill"
      variant="c"
      title="C. The session scoresheet"
      rationale="Each attempt is written into a small scoresheet beside the board, in pen or green, like a game score. Two tries; the sheet becomes the summary."
    >
      <div className="mx-auto flex max-w-7xl flex-col gap-14 px-(--space-gutter) py-10">
        <div className="flex max-w-prose flex-col gap-2">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            C. The session scoresheet
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            Each attempt is written into a small scoresheet beside the board, in pen or green, like
            a game score. Two tries; the sheet becomes the summary.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Position: DrNykterstein v ChessArtHouse (Lichess), after 63… Ka4. White mates in one
            with 64. Nb6#; the game went 64. Qc3, mate in three.
          </p>
        </div>
        <Sequence
          title="Wrong: written in pen, one more try"
          lede="The sheet keeps the history of the session, so a miss is visible but not loud."
          frames={C_WRONG}
        />
        <Sequence
          title="Right: written in green"
          lede="The streak lives in the sheet head; the finished sheet is the summary page."
          frames={C_RIGHT}
        />
      </div>
    </RoundPage>
  );
}
