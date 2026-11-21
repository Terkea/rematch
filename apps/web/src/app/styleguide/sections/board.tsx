"use client";

import { Board, EvalBar, NotationText, type Score, type Square } from "@ci/ui/board";
import { type GameFixture, games } from "@ci/ui/fixtures";
import { Button } from "@ci/ui/primitives";
import { Chess } from "chess.js";
import { useEffect, useMemo, useRef, useState } from "react";

// The board on real fixture moments, in both orientations. "Play the move" sets the position
// before the move, then plays it, so the piece's lift, travel, arc and landing can be watched.

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

function fenAt(game: GameFixture, ply: number): string {
  return ply === 0 ? game.startFen : (game.plies[ply - 1]?.fenAfter ?? game.startFen);
}

function scoreAt(game: GameFixture, ply: number): Score {
  const p = game.plies[ply - 1];
  if (!p) return { cp: 0 };
  return p.mateWhite !== null ? { mate: p.mateWhite } : { cp: p.cpWhite ?? 0 };
}

/** The king's square when the side to move is in check. */
function checkSquare(fen: string): Square | undefined {
  const chess = new Chess(fen);
  if (!chess.inCheck()) return undefined;
  const turn = chess.turn();
  for (const row of chess.board()) {
    for (const cell of row)
      if (cell?.type === "k" && cell.color === turn) return cell.square as Square;
  }
  return undefined;
}

interface MomentProps {
  game: GameFixture;
  ply: number;
  orientation?: "white" | "black";
  caption: string;
}

function Moment({ game, ply, orientation = "white", caption }: MomentProps) {
  const move = game.plies[ply - 1];
  if (!move) throw new Error(`No ply ${ply} in ${game.id}`);
  const [shown, setShown] = useState(ply);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const fen = fenAt(game, shown);
  const played = shown === ply;
  const replay = () => {
    if (timer.current) clearTimeout(timer.current);
    setShown(ply - 1);
    timer.current = setTimeout(() => setShown(ply), 400);
  };
  const lastMove = played
    ? { from: move.uci.slice(0, 2) as Square, to: move.uci.slice(2, 4) as Square }
    : undefined;
  const check = played ? checkSquare(fen) : undefined;
  return (
    <figure className="flex min-w-0 flex-col gap-3" data-moment={`${game.id}-${ply}`}>
      <div className="flex flex-col gap-2 md:flex-row md:gap-3">
        <EvalBar
          score={scoreAt(game, shown)}
          fen={fen}
          orientation={orientation}
          label="Evaluation"
          className="md:my-2"
        />
        <Board
          fen={fen}
          label={`${caption}, seen from ${orientation === "white" ? "White" : "Black"}`}
          orientation={orientation}
          {...(lastMove ? { lastMove } : {})}
          {...(check ? { check } : {})}
        />
      </div>
      <figcaption className="flex flex-wrap items-center justify-between gap-3 max-md:px-(--space-panel-pad)">
        <span className="text-base">
          <NotationText
            moveNumber={Math.ceil(ply / 2)}
            color={move.color}
            san={move.san}
            judgement={move.judgement}
            score={scoreAt(game, ply)}
          />
          <span className="ml-2 text-sm text-ink-soft">{caption}</span>
        </span>
        <Button variant="outline" size="sm" onClick={replay} data-replay>
          Play the move
        </Button>
      </figcaption>
    </figure>
  );
}

function PlayableBoard() {
  const [fen, setFen] = useState(START);
  const [lastMove, setLastMove] = useState<{ from: Square; to: Square } | undefined>();
  const check = useMemo(() => checkSquare(fen), [fen]);
  return (
    <figure className="flex min-w-0 flex-col gap-3" data-playable>
      <Board
        fen={fen}
        label="Practice board, White to play"
        interactive
        {...(lastMove ? { lastMove } : {})}
        {...(check ? { check } : {})}
        onMove={(m) => {
          const chess = new Chess(fen);
          try {
            chess.move(m);
          } catch {
            return false;
          }
          setFen(chess.fen());
          setLastMove({ from: m.from, to: m.to });
          return true;
        }}
      />
      <figcaption className="flex flex-wrap items-center justify-between gap-3 max-md:px-(--space-panel-pad)">
        <span className="text-sm text-ink-soft">
          Click, drag or use the keyboard: Tab to the board, arrows to move, Enter to pick up and
          drop, Escape to cancel.
        </span>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setFen(START);
            setLastMove(undefined);
          }}
        >
          Reset board
        </Button>
      </figcaption>
    </figure>
  );
}

const blunder = games.middlegameBlunder;
const attack = games.shortAttack;

export function BoardSection() {
  return (
    <section
      id="board"
      data-section="board"
      aria-labelledby="board-title"
      className="rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <h2 id="board-title" className="font-serif text-2xl font-semibold">
        Board
      </h2>
      <p className="mt-2 max-w-[66ch] text-sm text-ink-soft">
        Flat token squares, Cburnett pieces and coordinates on the squares. From 768px the board
        sits in an 8px well. Pieces carry their weight: a pawn is quick, a queen is heavy, and a
        knight jumps.
      </p>
      {/* Below 768px the board is flush (no well) and spans the sheet, as it spans the page. */}
      <div className="mt-6 flex flex-wrap gap-x-10 gap-y-12 max-md:-mx-(--space-panel-pad)">
        <Moment game={blunder} ply={55} caption="The knight retreats and drops the e-pawn" />
        <Moment game={blunder} ply={46} caption="A capture with check" />
        <Moment
          game={attack}
          ply={19}
          orientation="black"
          caption="Black's view: the knight takes on c6"
        />
        <Moment game={blunder} ply={15} caption="Castling: the king lands, then the rook" />
        <PlayableBoard />
      </div>
    </section>
  );
}
