import { RoundPage } from "../../_lib/page-guard";
import { PromotionColumn } from "../../_lib/promotion";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="promotion"
      variant="a"
      title="A. A column down the file"
      rationale="The four pieces stack down the h-file from the promotion square, queen first, over a dimmed board. Familiar from online play; one tap, no travel."
    >
      <div className="mx-auto flex max-w-6xl flex-col gap-8 px-(--space-gutter) py-10 lg:flex-row lg:items-start lg:gap-12">
        <div className="hidden md:block">
          <PromotionColumn px={448} />
        </div>
        <div className="md:hidden">
          <PromotionColumn px={352} />
        </div>
        <div className="flex max-w-md flex-col gap-3">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
            A. A column down the file
          </h1>
          <p className="m-0 text-prose text-ink-soft">
            The four pieces stack down the h-file from the promotion square, queen first, over a
            dimmed board. Familiar from online play; one tap, no travel.
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
