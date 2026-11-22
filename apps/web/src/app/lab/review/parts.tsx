"use client";

import { Board, EvalBar } from "@ci/ui/board";
import { EvalGraph } from "@ci/ui/eval-graph";
import { cn } from "@ci/ui/lib";
import { useSyncExternalStore } from "react";
import { accuracyLine, dayText, evalText, metaLine, moveText, resultLine } from "./model";
import type { LabReview } from "./use-lab-review";
import type { ReviewView as ReviewGame } from "./view";

// Pieces the three review variants share (lab only).

/** The h1 and its lines. `compact` keeps a phone's first viewport for the board. */
export function GameHeading({
  game,
  compact = false,
  brief = false,
  className,
}: {
  game: ReviewGame;
  compact?: boolean;
  /** Only the h1 and the meta line; the host shows `ResultLines` elsewhere. */
  brief?: boolean;
  className?: string;
}) {
  return (
    <header className={cn("flex min-w-0 flex-col gap-1", className)}>
      <h1
        className={cn(
          "font-serif font-semibold text-ink [overflow-wrap:anywhere]",
          compact ? "text-xl md:text-3xl" : "text-3xl",
        )}
      >
        {game.white.name} v {game.black.name}
      </h1>
      <p className="text-sm text-ink-soft">
        <time dateTime={game.playedAt.toISOString().slice(0, 10)}>{metaLine(game)}</time>
      </p>
      {brief ? null : <ResultLines game={game} className={cn(compact && "max-md:hidden")} />}
    </header>
  );
}

/** "You lost. <Opponent> won by checkmate" and the accuracy line. */
export function ResultLines({ game, className }: { game: ReviewGame; className?: string }) {
  const accuracy = accuracyLine(game);
  return (
    <div className={cn("flex flex-col gap-1 text-sm", className)}>
      <p className="text-ink">
        <span className="font-semibold">{resultLine(game)}</span>
        {game.termination ? <span className="text-ink-soft">. {game.termination}</span> : null}
      </p>
      {accuracy ? <p className="text-ink-soft">{accuracy}</p> : null}
    </div>
  );
}

/** The eval bar beside the board from 768px, above it on a phone. */
export function BoardStack({ r, className }: { r: LabReview; className?: string }) {
  return (
    <div className={cn("flex w-fit flex-col gap-2 md:flex-row md:gap-3", className)}>
      {r.score ? (
        <EvalBar
          score={r.score.score}
          fen={r.fen}
          orientation={r.orientation}
          label={`Evaluation, ${evalText(r.score)}`}
          className="md:my-2"
        />
      ) : null}
      <Board
        fen={r.fen}
        label={r.boardLabel}
        orientation={r.orientation}
        {...(r.lastMove ? { lastMove: r.lastMove } : {})}
        {...(r.check ? { check: r.check } : {})}
        annotations={r.annotations}
      />
    </div>
  );
}

export function Graph({ r, className }: { r: LabReview; className?: string }) {
  if (r.points.length === 0) return null;
  return (
    <EvalGraph
      className={className}
      points={r.points}
      label={`Win chance, ${r.title}`}
      currentPly={r.ply}
      onSelectPly={r.go}
      phases={r.game.phases}
      playerColor={r.game.playerColor}
    />
  );
}

/** The shown move in notation, with its eval: `31. Rxe6?? ≈ −4.2`. */
export function MoveReadout({ r, className }: { r: LabReview; className?: string }) {
  const mine = r.move?.color === r.game.playerColor;
  const judged = r.move?.analysis?.judgement;
  const ev = evalText(r.score);
  return (
    <p className={cn("flex items-baseline gap-3 font-serif", className)}>
      <span
        className={cn(
          "font-semibold [font-variant-numeric:lining-nums_tabular-nums]",
          judged ? (mine ? "text-pen-text" : "text-ink-soft") : "text-ink",
        )}
      >
        {r.moveText ?? "Start position"}
      </span>
      {ev ? (
        <span className="text-base text-ink-faint [font-variant-numeric:lining-nums_tabular-nums]">
          {ev}
        </span>
      ) : null}
    </p>
  );
}

/** The player's judged moves in game order, worst drop named, each a jump. */
export function MomentList({ r, className }: { r: LabReview; className?: string }) {
  if (r.moments.length === 0)
    return <p className={cn("text-sm text-ink-soft", className)}>No mistakes of yours to show.</p>;
  return (
    <ol aria-label="Your mistakes" className={cn("flex flex-col", className)}>
      {r.moments.map((ply) => {
        const p = r.game.plies[ply - 1];
        const a = p?.analysis;
        if (!p || !a || a.deltaW === undefined) return null;
        const after = p.color === "white" ? a.winWhite : 100 - a.winWhite;
        const before = after + a.deltaW;
        const current = ply === r.ply;
        return (
          <li key={ply}>
            <button
              type="button"
              onClick={() => r.go(ply)}
              aria-current={current ? "step" : undefined}
              className={cn(
                "flex min-h-11 w-full items-baseline justify-between gap-4 rounded-field px-3 text-left",
                "hover:bg-well aria-[current=step]:bg-well",
                "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
              )}
            >
              <span className="py-2 font-serif font-semibold text-pen-text [font-variant-numeric:lining-nums_tabular-nums]">
                {moveText(r.game, ply)}
              </span>
              <span className="text-sm text-ink-soft [font-variant-numeric:lining-nums_tabular-nums]">
                {Math.round(before)}% to {Math.round(after)}%
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

export { dayText };

const WIDE = "(min-width: 768px)";

/** True from 768px. False on the server and the first paint, so the narrow layout renders first. */
export function useWide(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia(WIDE);
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    },
    () => window.matchMedia(WIDE).matches,
    () => false,
  );
}
