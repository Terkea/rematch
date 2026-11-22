import { LabFrame } from "../../lab-frame";
import { loadLabReview } from "../load";
import { toReviewView } from "../view";
import { ReviewA } from "./view";

// Real data: the signed-in user's own game (?game=<id>), or their analysed game with the most
// mistakes. Dev-only: the layout and the loader 404 in production. The client view gets a narrow
// view model (view.ts), never the raw ReviewGame.
export const dynamic = "force-dynamic";

export default async function ReviewLabA({
  searchParams,
}: {
  searchParams: Promise<{ game?: string | string[] }>;
}) {
  const { game } = await searchParams;
  const review = await loadLabReview(typeof game === "string" ? game : undefined);
  return (
    <LabFrame
      surface="review"
      variant="a"
      title="Analysis desk"
      rationale="The board and the scoresheet side by side: the eval bar stands left of the board, the graph runs under it, and the scoresheet is a tall column on the right, as tall as the board, controls and graph beside it. The familiar analysis layout, with the sheet always in reach."
    >
      <ReviewA view={toReviewView(review, { drops: false })} />
    </LabFrame>
  );
}
