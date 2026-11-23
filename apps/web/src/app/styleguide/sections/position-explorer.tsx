"use client";

import { games } from "@ci/ui/fixtures";
import {
  type ExplorerMove,
  moveFrom,
  PositionExplorer,
  type PositionLine,
} from "@ci/ui/position-explorer";
import { TooltipProvider } from "@ci/ui/primitives";

// The position explorer (feat/position-explorer) over the fixture blitz game DrNykterstein v
// Night-King96: 28. Nd2?? with eight plies of the game either side, and 1. d4, where the fixture's
// analysis preferred 1. e4. The fixture stores no engine line beyond that one move, so the first
// demo shows the honest "no engine line" state and the second a one-move engine line.

const game = games.middlegameBlunder;
const fenBefore = (ply: number) =>
  ply === 1 ? game.startFen : (game.plies[ply - 2]?.fenAfter ?? game.startFen);

function windowOf(ply: number, n: number): ExplorerMove[] {
  const out: ExplorerMove[] = [];
  for (let p = Math.max(1, ply - n); p <= Math.min(game.plies.length, ply + n); p++) {
    const m = game.plies[p - 1];
    if (!m) continue;
    out.push({
      ply: p,
      color: m.color,
      san: m.san,
      uci: m.uci,
      fenBefore: fenBefore(p),
      fenAfter: m.fenAfter,
    });
  }
  return out;
}

const nd2Line: PositionLine = { game: windowOf(55, 8), engine: [] };

const first = game.plies[0];
const e4 = moveFrom(game.startFen, "e2e4", 1);
const d4Line: PositionLine = { game: windowOf(1, 8), engine: e4 ? [e4] : [] };

const API: [string, string][] = [
  ["gameId", "The game as loadLine knows it: the review's game id (games.id) in the web app."],
  ["ply", "The cited move's ply (1 is White's first move)."],
  ["fenBefore", "The position before the cited move."],
  ["played", "{ san, uci } of the move played: a pen arrow for the player's mistake, else pencil."],
  ["best", "{ san, uci } of the engine's better move, or null: a good arrow."],
  ["orientation", "The player's side; the expanded board can be flipped."],
  ["label", "The cited move as notation, 28. Nd2 or 28… Nd2."],
  ["mistake", "The player's own mistake or blunder."],
  [
    "found",
    "The move played was the engine's own (a strength): a good arrow with a ! badge, named in the key.",
  ],
  [
    "href",
    "The review at that ply: Open the game, and Open full game. Without one there is no link.",
  ],
  [
    "loadLine",
    "(gameId, ply) => Promise<PositionLine | null>, called once on first expand. The web app passes fetchPositionLine (lib/position-line.ts, GET /api/games/<id>/line?ply=N).",
  ],
  ["line", "The expanded data when the host already has it (no fetch)."],
];

export function PositionExplorerSection() {
  return (
    <TooltipProvider>
      <section
        id="position-explorer"
        data-section="position-explorer"
        aria-labelledby="position-explorer-title"
        className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
      >
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="position-explorer-title" className="font-serif text-2xl font-semibold">
            Position explorer
          </h2>
          <p className="text-sm text-ink-soft">
            One cited position, anywhere a page names a move: the small board with the move played
            and the engine&apos;s better move. Expand it (or click the board) to step through the
            game around it with the buttons or the arrow keys, switch to the engine&apos;s stored
            line, or try moves of your own. Moves you try are never given an evaluation: no engine
            checks them here.
          </p>
        </div>

        <div className="flex flex-wrap gap-8">
          <PositionExplorer
            gameId={game.id}
            ply={55}
            fenBefore={fenBefore(55)}
            played={{ san: "Nd2", uci: "e4d2" }}
            best={null}
            orientation={game.playerColor}
            label="28. Nd2"
            mistake
            judgement="blunder"
            href="#position-explorer"
            line={nd2Line}
          />
          {first ? (
            <PositionExplorer
              gameId={game.id}
              ply={1}
              fenBefore={game.startFen}
              played={{ san: first.san, uci: first.uci }}
              best={{ san: "e4", uci: "e2e4" }}
              orientation={game.playerColor}
              label="1. d4"
              href="#position-explorer"
              line={d4Line}
            />
          ) : null}
        </div>

        <div className="flex flex-col gap-3">
          <h3 className="m-0 font-serif text-xl font-semibold">Props</h3>
          <p className="m-0 max-w-prose text-sm text-ink-soft">
            From <code className="font-sans font-medium text-ink">@ci/ui/position-explorer</code>.
            The coach&apos;s answers use it through CitedBoard; weakness examples and the strengths
            page use it through GamePosition (components/game-position.tsx).
          </p>
          <dl className="m-0 grid max-w-prose grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-[auto_1fr]">
            {API.map(([name, text]) => (
              <div key={name} className="contents">
                <dt className="font-sans text-sm font-medium text-ink">{name}</dt>
                <dd className="m-0 font-sans text-sm text-ink-soft">{text}</dd>
              </div>
            ))}
          </dl>
        </div>
      </section>
    </TooltipProvider>
  );
}
