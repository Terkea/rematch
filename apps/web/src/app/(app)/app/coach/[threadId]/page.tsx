import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { cache } from "react";
import { segmentGameIds } from "../../../../../lib/segment-href";
import { requireUser } from "../../../../../server/auth/session";
import { chatAllowance } from "../../../../../server/coach/allowance";
import { coachStatus } from "../../../../../server/coach/status";
import { getThread, listConversations, type Thread } from "../../../../../server/coach/threads";
import { getDb } from "../../../../../server/db";
import { requestEventCursor } from "../../../../../server/events/request-cursor";
import { reviewGameIds } from "../../../../../server/profile/game-ids";
import { allowanceView } from "../allowance";
import { ThreadList } from "../thread-list";
import { AnchorCard, loadAnchorView } from "./anchor-card";
import { ThreadView } from "./thread-view";

export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ threadId: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
};

/** One read per request, shared by the metadata and the page. Not the user's: not found. */
const load = cache(async (threadId: string): Promise<{ userId: string; thread: Thread }> => {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const thread = await getThread(getDb(), user.id, threadId);
  if (!thread) notFound();
  return { userId: user.id, thread };
});

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { threadId } = await params;
  return { title: (await load(threadId)).thread.title };
}

/**
 * `/app/coach/[threadId]`: one of the user's own conversations. A reload mid-answer keeps what
 * already streamed (`getThread` replays the stored chunks, R5); the rest arrives on the stream.
 */
export default async function ThreadPage({ params }: Props) {
  const { threadId } = await params;
  const { userId, thread } = await load(threadId);
  const db = getDb();
  // Answers name games by platform id; their links need `games.id` (on this account only).
  const named = thread.messages.flatMap((m) => [
    ...segmentGameIds(m.segments),
    ...m.streamed.flatMap((c) => segmentGameIds(c.segments)),
  ]);
  const [allowance, threads, anchor, ids, status] = await Promise.all([
    chatAllowance(db, thread.accountId),
    listConversations(db, userId, thread.accountId),
    thread.anchor
      ? loadAnchorView(db, userId, thread.accountId, thread.anchor)
      : Promise.resolve(null),
    reviewGameIds(db, userId, { accountId: thread.accountId }, [...new Set(named)]),
    coachStatus(db),
  ]);
  const q = new URLSearchParams({ account: thread.accountId, tc: thread.timeClass });
  return (
    <ThreadView
      key={thread.id}
      thread={thread}
      allowance={allowanceView(allowance)}
      listHref={`/app/coach?${q.toString()}`}
      available={status === "available"}
      gameIds={Object.fromEntries(ids)}
      anchorCard={anchor ? <AnchorCard view={anchor} /> : null}
      aside={
        threads.length > 1 ? (
          <div data-surface="sheet" className="rounded-sheet bg-sheet py-1">
            <ThreadList threads={threads} currentId={thread.id} compact />
          </div>
        ) : null
      }
    />
  );
}
