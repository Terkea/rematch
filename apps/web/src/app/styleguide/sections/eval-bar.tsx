"use client";

import { EvalBar, NotationText, type Score } from "@ci/ui/board";
import { games } from "@ci/ui/fixtures";
import { Button } from "@ci/ui/primitives";
import { useState } from "react";

// The eval bar on real evaluations. The stepper walks through the middlegame blunder game: small
// changes move on `swing`, and 28. Nd2?? (a loss of more than 0.3 win chance) swings past on
// `swing-big` and comes back.

const game = games.middlegameBlunder;
const FIRST = 50;
const LAST = 62;

const scoreOf = (ply: number): Score => {
  const p = game.plies[ply - 1];
  if (!p) return { cp: 0 };
  return p.mateWhite !== null ? { mate: p.mateWhite } : { cp: p.cpWhite ?? 0 };
};

const samples: { score: Score; text: string }[] = [
  { score: { cp: 0 }, text: "Level" },
  { score: { cp: 35 }, text: "A small edge" },
  { score: { cp: -420 }, text: "Black is winning" },
  { score: { mate: 2 }, text: "White mates in 2" },
];

export function EvalBarSection() {
  const [ply, setPly] = useState(54);
  const move = game.plies[ply - 1];
  return (
    <section
      id="eval-bar"
      data-section="eval-bar"
      aria-labelledby="eval-bar-title"
      className="rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <h2 id="eval-bar-title" className="font-serif text-2xl font-semibold">
        Eval bar
      </h2>
      <p className="mt-2 max-w-[66ch] text-sm text-ink-soft">
        White's share on the win-chance scale of the eval graph. 16px and the board's height from
        768px, an 8px bar above the board below that. It flips with the board.
      </p>

      <div
        className="mt-6 flex flex-col gap-3 max-md:[--board-size:100%] md:[--board-size:320px]" /* ban-lint-allow TOK2: styleguide demo sets the --board-size token itself */
      >
        <h3 className="font-serif text-xl font-semibold">Swing</h3>
        <div className="flex flex-col gap-4 md:flex-row md:items-end">
          <div className="flex gap-3 max-md:flex-col">
            <EvalBar score={scoreOf(ply)} label="Evaluation, seen from White" />
            <EvalBar score={scoreOf(ply)} orientation="black" label="Evaluation, seen from Black" />
          </div>
          <div className="flex flex-col gap-3">
            {move ? (
              <p className="text-base" aria-live="polite">
                <NotationText
                  moveNumber={Math.ceil(ply / 2)}
                  color={move.color}
                  san={move.san}
                  judgement={move.judgement}
                  side={move.color === game.playerColor ? "player" : "opponent"}
                  score={scoreOf(ply)}
                />
              </p>
            ) : null}
            <div className="flex gap-3">
              <Button
                variant="outline"
                size="sm"
                disabled={ply <= FIRST}
                onClick={() => setPly((p) => Math.max(FIRST, p - 1))}
              >
                Previous move
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={ply >= LAST}
                onClick={() => setPly((p) => Math.min(LAST, p + 1))}
                data-next-move
              >
                Next move
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div
        className="mt-8 flex flex-col gap-3 max-md:[--board-size:100%] md:[--board-size:192px]" /* ban-lint-allow TOK2: styleguide demo sets the --board-size token itself */
      >
        <h3 className="font-serif text-xl font-semibold">Values</h3>
        <ul className="flex flex-col gap-4 md:flex-row md:gap-8">
          {samples.map(({ score, text }) => (
            <li key={text} className="flex items-center gap-3 md:flex-col md:items-start">
              <EvalBar score={score} label={`Evaluation, ${text.toLowerCase()}`} />
              <span className="text-sm text-ink-soft max-md:hidden">{text}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
