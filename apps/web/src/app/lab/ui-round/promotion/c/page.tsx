import { RoundPage } from "../../_lib/page-guard";
import { PromotionBelow } from "../../_lib/promotion";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="promotion"
      variant="c"
      title="C. Large buttons under the board"
      rationale="The board keeps a ghost pawn on h8 and the choice sits under it as four named buttons. Biggest targets on a phone; the eye has to leave the square."
    >
      <div className="mx-auto flex max-w-6xl flex-col gap-8 px-(--space-gutter) py-10 lg:flex-row lg:items-start lg:gap-12">
        <div className="hidden md:block">
          <PromotionBelow px={448} />
        </div>
        <div className="md:hidden">
          <PromotionBelow px={352} />
        </div>
        <div className="flex max-w-md flex-col gap-3">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            C. Large buttons under the board
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            The board keeps a ghost pawn on h8 and the choice sits under it as four named buttons.
            Biggest targets on a phone; the eye has to leave the square.
          </p>
          <p className="m-0 text-sm text-ink-soft">
            Keyboard: 1 to 4 or the arrow keys and Enter; Escape takes the pawn back. Today every
            promotion becomes a queen without asking.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Position: DrNykterstein v ChessArtHouse (Lichess), 60. h8=Q in the game.
          </p>
        </div>
      </div>
    </RoundPage>
  );
}
