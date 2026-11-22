import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { cache } from "react";
import { requireUser } from "../../../../../server/auth/session";
import { getDb } from "../../../../../server/db";
import { getGameReview, type ReviewGame } from "../../../../../server/games/review";
import { accuracyLine, metaLine, pageTitle, resultLine } from "./header";
import { GameReviewView } from "./review-view";
import { toReviewView } from "./view-model";

// One of the user's games, stepped move by move (M5a T10). Scope, the analysis guard and every
// not-found case are T3's `getGameReview`: another user's game, an unknown or a malformed id all
// give the same not-found. The header is rendered here; the client view gets the narrow model.

export const dynamic = "force-dynamic";

type Props = {
  params: Promise<{ gameId: string }>;
  searchParams: Promise<{ ply?: string | string[] }>;
};

/** One read per request, shared by the metadata and the page. */
const load = cache(async (gameId: string): Promise<ReviewGame> => {
  const user = await requireUser();
  const game = await getGameReview(getDb(), user.id, gameId);
  if (!game) notFound();
  return game;
});

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { gameId } = await params;
  return { title: pageTitle(await load(gameId)) };
}

export default async function GameReviewPage({ params, searchParams }: Props) {
  const { gameId } = await params;
  const { ply } = await searchParams;
  const game = await load(gameId);
  // One column on a phone (title, review, then the result, so the board and the verdict make
  // the first screen); from 768px the result sits beside the title.
  return (
    <div className="grid gap-x-8 gap-y-5 md:grid-cols-[minmax(0,1fr)_auto] md:gap-y-6">
      <ReviewHeader game={game} />
      <ResultLines game={game} />
      <div className="md:col-span-2">
        <GameReviewView
          view={toReviewView(game)}
          {...(typeof ply === "string" ? { initialPly: ply } : {})}
        />
      </div>
    </div>
  );
}

function ReviewHeader({ game }: { game: ReviewGame }) {
  return (
    <header className="flex min-w-0 flex-col gap-1 md:self-end">
      <h1 className="font-serif text-xl font-semibold text-ink [overflow-wrap:anywhere] md:text-2xl">
        {game.white.name} v {game.black.name}
      </h1>
      <p className="text-sm text-ink-soft">
        <time dateTime={game.playedAt.toISOString().slice(0, 10)}>{metaLine(game)}</time>
        {/* The phone's first screen says who won; the full result lines close the page there. */}
        <span className="font-semibold text-ink md:hidden">, {RESULT_WORD[game.playerResult]}</span>
      </p>
    </header>
  );
}

const RESULT_WORD: Record<ReviewGame["playerResult"], string> = {
  win: "won",
  loss: "lost",
  draw: "drawn",
  unfinished: "unfinished",
};

/** "You lost. <Opponent> won by checkmate", the accuracies and the link to the game. */
function ResultLines({ game }: { game: ReviewGame }) {
  const accuracy = accuracyLine(game);
  return (
    <section
      aria-label="Result"
      className="flex flex-col gap-1 text-sm max-md:order-last md:items-end md:self-end md:text-right"
    >
      <p className="text-ink">
        <span className="font-semibold">{resultLine(game)}</span>
        {game.termination ? (
          <>
            <span className="text-ink-soft">. </span>
            <span className="text-ink-soft">{game.termination}</span>
          </>
        ) : null}
      </p>
      {accuracy.length > 0 || game.sourceUrl ? (
        <p className="flex flex-wrap gap-x-4 gap-y-1 text-ink-soft md:justify-end">
          {accuracy.map((a) => (
            <span key={a}>{a}</span>
          ))}
          {game.sourceUrl ? (
            <a
              href={game.sourceUrl}
              target="_blank"
              rel="noreferrer noopener"
              className="text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink"
            >
              View on {game.platform === "lichess" ? "lichess" : "chess.com"}
            </a>
          ) : null}
        </p>
      ) : null}
    </section>
  );
}
