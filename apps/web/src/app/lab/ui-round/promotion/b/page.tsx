import { RoundPage } from "../../_lib/page-guard";
import { PromotionFan } from "../../_lib/promotion";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="promotion"
      variant="b"
      title="B. A labelled row beside the square"
      rationale="A small floating row next to h8 with the piece names and number keys 1 to 4. The board stays readable; slower to hit on a phone."
    >
      <div className="mx-auto flex max-w-6xl flex-col gap-8 px-(--space-gutter) py-10 lg:flex-row lg:items-start lg:gap-12">
        <div className="hidden md:block">
          <PromotionFan px={448} />
        </div>
        <div className="md:hidden">
          <PromotionFan px={352} />
        </div>
        <div className="flex max-w-md flex-col gap-3">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            B. A labelled row beside the square
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            A small floating row next to h8 with the piece names and number keys 1 to 4. The board
            stays readable; slower to hit on a phone.
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
