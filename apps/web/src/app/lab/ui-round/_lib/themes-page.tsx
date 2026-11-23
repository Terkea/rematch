"use client";

import { Board } from "@ci/ui/board";
import {
  type BoardColouring,
  COLOURINGS,
  ColouringStyles,
  colouringClass,
  PieceRow,
  StaticBoard,
} from "./board/static-board";
import { BUSY_FEN, BUSY_LAST, DRILL } from "./fixtures";

const CONTRAST: Record<BoardColouring, { day: string; night: string }> = {
  slate: {
    day: "squares 1.71:1; black pieces 7.47:1 on dark squares; white pieces' outline 12.75:1 on light",
    night: "squares 1.82:1; black pieces 3.40:1 on dark squares, 6.20:1 on light",
  },
  paper: {
    day: "squares 1.81:1; black pieces 8.45:1 on dark squares; white pieces' outline 15.28:1 on light",
    night: "squares 1.61:1; black pieces 4.91:1 on dark squares, 7.89:1 on light",
  },
  pencil: {
    day: "squares 2.88:1; black pieces 4.55:1 on dark squares; white pieces' outline 13.12:1 on light",
    night: "squares 2.39:1; black pieces 3.44:1 on dark squares, 8.21:1 on light",
  },
};

export function ThemesPage({ colouring, letter }: { colouring: BoardColouring; letter: string }) {
  const c = COLOURINGS[colouring];
  return (
    <div className="mx-auto flex max-w-7xl flex-col gap-10 px-(--space-gutter) py-10">
      <ColouringStyles />
      <div className="flex max-w-prose flex-col gap-2">
        <h1 className="m-0 font-serif text-3xl font-semibold text-ink">
          {letter}. {c.name} squares
        </h1>
        <p className="m-0 text-prose text-ink-soft">{c.note}</p>
        <p className="m-0 text-sm text-ink-soft">
          Made from: day {c.day}; night {c.night}. No new colour.
        </p>
        <p className="m-0 text-sm text-ink-faint">
          Day: {CONTRAST[colouring].day}. Night: {CONTRAST[colouring].night}.
        </p>
      </div>
      <div className="grid grid-cols-1 gap-10 xl:grid-cols-2">
        <figure className={`m-0 flex flex-col gap-3 ${colouringClass(colouring)}`}>
          <Board
            fen={DRILL.fen}
            label="Cburnett, with a drill's marks"
            orientation="white"
            lastMove={DRILL.lastMove}
            animateAnnotations={false}
            annotations={[
              { kind: "arrow", from: "d3", to: "c3", tone: "pen", toEdge: true },
              { kind: "mark", square: "c3", glyph: "?", tone: "pen" },
              { kind: "arrow", from: "d5", to: "b6", tone: "good" },
              { kind: "mark", square: "b6", glyph: "!", tone: "good" },
            ]}
          />
          <figcaption className="text-sm text-ink-soft">
            Cburnett with the drill's marks: your move in pen, the answer in green.
          </figcaption>
        </figure>
        <figure className="m-0 flex flex-col gap-3">
          <div className="md:hidden">
            <StaticBoard
              fen={BUSY_FEN}
              set="chessnut"
              colouring={colouring}
              px={352}
              lastMove={BUSY_LAST}
              label="Chessnut pieces"
            />
          </div>
          <div className="hidden md:block">
            <StaticBoard
              fen={BUSY_FEN}
              set="chessnut"
              colouring={colouring}
              px={480}
              lastMove={BUSY_LAST}
              label="Chessnut pieces"
            />
          </div>
          <figcaption className="text-sm text-ink-soft">
            Chessnut on the same squares, in a busy middlegame (after 35… Qxf2 in a fixture game).
          </figcaption>
        </figure>
      </div>
      <div className="flex flex-col gap-3">
        <h2 className="m-0 font-serif text-xl font-semibold text-ink">The Settings rows</h2>
        <div className="flex flex-wrap gap-6">
          <PieceRow set="cburnett" colouring={colouring} sq={40} />
          <PieceRow set="chessnut" colouring={colouring} sq={40} />
        </div>
      </div>
    </div>
  );
}
