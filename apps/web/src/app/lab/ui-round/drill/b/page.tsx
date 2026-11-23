import { B_RIGHT, B_WRONG, Sequence } from "../../_lib/drill";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="drill"
      variant="b"
      title="B. One attempt, the answer at once"
      rationale="No retry: a wrong move springs back and both arrows draw, yours in pen and the answer in green. Progress is a strip of eight result squares instead of a streak."
    >
      <div className="mx-auto flex max-w-7xl flex-col gap-14 px-(--space-gutter) py-10">
        <div className="flex max-w-prose flex-col gap-2">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            B. One attempt, the answer at once
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            No retry: a wrong move springs back and both arrows draw, yours in pen and the answer in
            green. Progress is a strip of eight result squares instead of a streak.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Position: DrNykterstein v ChessArtHouse (Lichess), after 63… Ka4. White mates in one
            with 64. Nb6#; the game went 64. Qc3, mate in three.
          </p>
        </div>
        <Sequence
          title="Wrong: refused, then shown"
          lede="Faster sessions and no guessing loop; the retry moves to tomorrow (spaced repetition)."
          frames={B_WRONG}
        />
        <Sequence
          title="Right: ring and !, the strip fills"
          lede="No streak number; the strip shows the whole session at a glance."
          frames={B_RIGHT}
        />
      </div>
    </RoundPage>
  );
}
