import { A_RIGHT, A_WRONG, Sequence } from "../../_lib/drill";
import { RoundPage } from "../../_lib/page-guard";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="drill"
      variant="a"
      title="A. Spring back, one more try"
      rationale="The shipped spec, finished: a wrong move springs back with a pen ? and a second try (with a hint on offer); a right move gets the good ring, the ! and a streak number."
    >
      <div className="mx-auto flex max-w-7xl flex-col gap-14 px-(--space-gutter) py-10">
        <div className="flex max-w-prose flex-col gap-2">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            A. Spring back, one more try
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            The shipped spec, finished: a wrong move springs back with a pen ? and a second try
            (with a hint on offer); a right move gets the good ring, the ! and a streak number.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Position: DrNykterstein v ChessArtHouse (Lichess), after 63… Ka4. White mates in one
            with 64. Nb6#; the game went 64. Qc3, mate in three.
          </p>
        </div>
        <Sequence
          title="Wrong: the move is refused, then one retry"
          lede="The board never keeps a wrong move. A second try trains recall instead of reading the answer."
          frames={A_WRONG}
        />
        <Sequence
          title="Right: ring, !, streak"
          lede="The streak is a number that lands; the margin note is the only hand-made touch."
          frames={A_RIGHT}
        />
      </div>
    </RoundPage>
  );
}
