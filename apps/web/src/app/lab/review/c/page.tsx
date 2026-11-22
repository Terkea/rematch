import { LabFrame } from "../../lab-frame";
import { loadLabReview } from "../load";
import { toReviewView } from "../view";
import { ReviewC } from "./view";

// Real data: the signed-in user's own game (?game=<id>), or their analysed game with the most
// mistakes. Dev-only: the layout and the loader 404 in production. The client view gets a narrow
// view model (view.ts), never the raw ReviewGame.
export const dynamic = "force-dynamic";

export default async function ReviewLabC({
  searchParams,
}: {
  searchParams: Promise<{ game?: string | string[] }>;
}) {
  const { game } = await searchParams;
  const review = await loadLabReview(typeof game === "string" ? game : undefined);
  return (
    <LabFrame
      surface="review"
      variant="c"
      title="Annotated sheet"
      rationale="The scoresheet leads, read like a coach's sheet: the shown move in large notation with its commentary line, then the player's mistakes in game order, the ruled moves and the graph as the sheet's footer. The board floats beside it and stays in view while the sheet scrolls."
    >
      <ReviewC view={toReviewView(review, { drops: true })} />
    </LabFrame>
  );
}
