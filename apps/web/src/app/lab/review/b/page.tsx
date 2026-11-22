import { LabFrame } from "../../lab-frame";
import { loadLabReview } from "../load";
import { toReviewView } from "../view";
import { ReviewB } from "./view";

// Real data: the signed-in user's own game (?game=<id>), or their analysed game with the most
// mistakes. Dev-only: the layout and the loader 404 in production. The client view gets a narrow
// view model (view.ts), never the raw ReviewGame.
export const dynamic = "force-dynamic";

export default async function ReviewLabB({
  searchParams,
}: {
  searchParams: Promise<{ game?: string | string[] }>;
}) {
  const { game } = await searchParams;
  const review = await loadLabReview(typeof game === "string" ? game : undefined);
  return (
    <LabFrame
      surface="review"
      variant="b"
      title="Graph as the spine"
      rationale="The win-chance graph runs full width on top and drives the board: every swing is visible before the first move is chosen. The board, the shown move and its coach's line sit under it; on phones the scoresheet moves into a drawer so the graph, board and controls share the first screen."
    >
      <ReviewB view={toReviewView(review, { drops: true })} />
    </LabFrame>
  );
}
