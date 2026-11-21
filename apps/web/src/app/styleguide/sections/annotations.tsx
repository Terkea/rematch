"use client";

import type { Annotation } from "@ci/ui/annotations";
import { Board, NotationText, type Score, type Square } from "@ci/ui/board";
import { games } from "@ci/ui/fixtures";
import { Button } from "@ci/ui/primitives";
import { useState } from "react";

// The red pen on the board. 28. Nd2?? from the middlegame blunder game: the played move as a pen
// arrow (a knight's L), Stockfish's 28. Kd2 as a good arrow, a pen ring on d2 and the ?? badge.
// Beside it, the opponent's blunder 31…Nf4?? in pencil (Task 13 ruling: the opponent's marks are
// pencil, so red always means "your mistake"). "Replay marks" draws them on again.

const game = games.middlegameBlunder;

/** Stockfish 17 at depth 22 on the position before 28. Nd2 (the fixture carries no best move). */
const BEST_28 = { from: "e2", to: "d2" } as const satisfies { from: Square; to: Square };

const scoreOf = (ply: number): Score => {
  const p = game.plies[ply - 1];
  if (!p) return { cp: 0 };
  return p.mateWhite !== null ? { mate: p.mateWhite } : { cp: p.cpWhite ?? 0 };
};

function squares(uci: string) {
  return { from: uci.slice(0, 2) as Square, to: uci.slice(2, 4) as Square };
}

interface MomentProps {
  ply: number;
  annotations: Annotation[];
  caption: string;
  replay: number;
}

function Moment({ ply, annotations, caption, replay }: MomentProps) {
  const move = game.plies[ply - 1];
  if (!move) throw new Error(`No ply ${ply}`);
  return (
    <figure className="flex min-w-0 flex-col gap-3" data-annotated={ply}>
      <Board
        key={replay}
        fen={move.fenAfter}
        label={`Position after ${Math.ceil(ply / 2)}${move.color === "white" ? "." : "…"} ${move.san}`}
        lastMove={squares(move.uci)}
        annotations={annotations}
      />
      <figcaption className="flex flex-col gap-1 max-md:px-(--space-panel-pad)">
        <span className="text-base">
          <NotationText
            moveNumber={Math.ceil(ply / 2)}
            color={move.color}
            san={move.san}
            judgement={move.judgement}
            side={move.color === game.playerColor ? "player" : "opponent"}
            score={scoreOf(ply)}
          />
        </span>
        <span className="max-w-[44ch] text-sm text-ink-soft">{caption}</span>
      </figcaption>
    </figure>
  );
}

const played = squares(game.plies[54]?.uci ?? "e4d2");
const blunder: Annotation[] = [
  { kind: "circle", square: played.to, tone: "pen" },
  { kind: "arrow", ...played, tone: "pen" },
  { kind: "arrow", ...BEST_28, tone: "good" },
  { kind: "mark", square: played.to, glyph: "??", tone: "pen" },
];

const theirs = squares(game.plies[61]?.uci ?? "h5f4");
const opponent: Annotation[] = [
  { kind: "circle", square: theirs.to, tone: "pencil" },
  { kind: "arrow", ...theirs, tone: "pencil" },
  { kind: "mark", square: theirs.to, glyph: "??", tone: "pencil" },
];

export function AnnotationsSection() {
  const [replay, setReplay] = useState(0);
  return (
    <section
      id="annotations"
      data-section="annotations"
      aria-labelledby="annotations-title"
      className="rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h2 id="annotations-title" className="font-serif text-2xl font-semibold">
          Annotations
        </h2>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setReplay((n) => n + 1)}
          data-replay-marks
        >
          Replay marks
        </Button>
      </div>
      <p className="mt-2 max-w-[66ch] text-sm text-ink-soft">
        A coach's pen, not a vector tool. Every stroke is a tapered outline on a sheet halo, seeded
        so it looks the same on every visit, and drawn on in order: the ring, the move played, the
        better move, then the badge lands. On the board, red is only ever your mistake; the
        opponent's is pencil.
      </p>
      <div className="mt-6 flex flex-wrap gap-x-10 gap-y-12 max-md:-mx-(--space-panel-pad)">
        <Moment
          ply={55}
          annotations={blunder}
          replay={replay}
          caption="The knight steps back and drops the e-pawn. The green arrow is Stockfish's choice, 28. Kd2."
        />
        <Moment
          ply={62}
          annotations={opponent}
          replay={replay}
          caption="The opponent's blunder a few moves later. On the board it is pencil, so a red mark there is always yours."
        />
      </div>
    </section>
  );
}
