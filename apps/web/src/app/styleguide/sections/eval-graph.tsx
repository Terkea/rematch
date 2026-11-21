"use client";

import { EvalGraph, type EvalGraphPoint, plyText } from "@ci/ui/eval-graph";
import { type GameFixture, games } from "@ci/ui/fixtures";
import { useMemo, useState } from "react";

// The eval graph on the three real fixture games. The first is linked to a notation readout
// through `currentPly` (the Board lane replaces the readout once it lands).

const toPoints = (game: GameFixture): EvalGraphPoint[] =>
  game.plies.map((p) => ({ ply: p.ply, winWhite: p.winWhite, san: p.san, judgement: p.judgement }));

const phasesOf = (game: GameFixture) => ({
  middlegameStartPly: game.plies.find((p) => p.phase === "middlegame")?.ply,
  endgameStartPly: game.plies.find((p) => p.phase === "endgame")?.ply,
});

const title = (game: GameFixture) => `${game.white.name} v ${game.black.name}, ${game.result}`;

const firstSerious = (game: GameFixture) =>
  game.plies.find((p) => p.judgement === "blunder" || p.judgement === "mistake")?.ply ?? 1;

function GameGraph({
  game,
  linked = false,
  run,
}: {
  game: GameFixture;
  linked?: boolean;
  run: number;
}) {
  const [ply, setPly] = useState(() => firstSerious(game));
  const points = useMemo(() => toPoints(game), [game]);
  const phases = useMemo(() => phasesOf(game), [game]);
  const current = points.find((p) => p.ply === ply);
  return (
    <div className="flex flex-col gap-2">
      <h3 className="font-serif text-xl font-semibold">{title(game)}</h3>
      <p className="text-sm text-ink-soft">
        {game.openingName ?? "Unnamed opening"}, {game.timeClass}, {game.plies.length} half-moves.
      </p>
      <EvalGraph
        key={run}
        drawIn={run > 0}
        points={points}
        label={`Win chance, ${title(game)}`}
        phases={phases}
        playerColor={game.playerColor}
        {...(linked ? { currentPly: ply, onSelectPly: setPly } : {})}
      />
      {linked && current && (
        // Not a live region: the graph's slider already announces the move.
        <p className="text-sm text-ink-soft">
          Showing {plyText(current).replace(/^Move/, "move")}.
        </p>
      )}
    </div>
  );
}

export function EvalGraphSection() {
  const [run, setRun] = useState(0);
  return (
    <section
      id="eval-graph"
      data-section="eval-graph"
      aria-labelledby="eval-graph-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="eval-graph-title" className="font-serif text-2xl font-semibold">
            Eval graph
          </h2>
          <p className="text-sm text-ink-soft">
            White's win chance after every move. Long pen ticks with ? or ?? mark mistakes and
            blunders, and short pencil ticks mark inaccuracies. Focus a graph and use the arrow
            keys, Home and End to step through the game.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setRun((n) => n + 1)}
          className="rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Replay draw
        </button>
      </div>
      <GameGraph game={games.middlegameBlunder} linked run={run} />
      <GameGraph game={games.endgameGrind} run={run} />
      <GameGraph game={games.shortAttack} run={run} />
    </section>
  );
}
