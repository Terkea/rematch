"use client";

import { Board } from "@ci/ui/board";
import type { ReactNode } from "react";
import { BoardOverlay, BoardSize, OverlayPiece, SetPiece } from "./board/static-board";
import { PROMOTION } from "./fixtures";

// Three promotion pickers over the real Board: the pawn has been dropped on h8 and the player
// chooses the piece. The position is from the endgame-grind fixture (the game played h8=Q).

const PIECES = [
  { code: "wQ", name: "Queen", key: "1" },
  { code: "wN", name: "Knight", key: "2" },
  { code: "wR", name: "Rook", key: "3" },
  { code: "wB", name: "Bishop", key: "4" },
] as const;

const WITHOUT_PAWN = "8/8/6K1/8/kB3N2/P7/8/8 w - - 1 60";

function Scene({
  px,
  cells,
  scrim = false,
  children,
  below,
}: {
  px: number;
  cells: Record<string, ReactNode>;
  scrim?: boolean;
  children?: ReactNode;
  below?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-4">
      <BoardSize px={px} className="relative w-fit">
        <div className="relative">
          <Board
            fen={WITHOUT_PAWN}
            label={PROMOTION.label}
            orientation="white"
            lastMove={{ from: "h7", to: "h8" }}
            className="md:bg-transparent md:p-0"
          />
          {scrim ? (
            <div
              aria-hidden="true"
              className="absolute inset-0 z-[4] rounded-board bg-paper opacity-40"
            />
          ) : null}
          <BoardOverlay cells={cells} />
          {children}
        </div>
      </BoardSize>
      {below}
    </div>
  );
}

/** A: the four choices stacked down the h-file from the promotion square; the rest dims. */
export function PromotionColumn({ px }: { px: number }) {
  const squares = ["h8", "h7", "h6", "h5"];
  const cells: Record<string, ReactNode> = {};
  PIECES.forEach((p, i) => {
    cells[squares[i] as string] = (
      <span
        className={`absolute inset-0.5 rounded-field border bg-sheet shadow-lift ${i === 0 ? "border-ink" : "border-rule-strong"}`}
        title={`${p.name} (${p.key})`}
      >
        <OverlayPiece code={p.code} />
      </span>
    );
  });
  return (
    <Scene px={px} cells={cells} scrim>
      <p className="sr-only">
        Choose a piece: queen, knight, rook or bishop. Escape takes the pawn back.
      </p>
    </Scene>
  );
}

/** B: a small floating row anchored beside the square, with names and number keys. */
export function PromotionFan({ px }: { px: number }) {
  return (
    <Scene px={px} cells={{ h8: <OverlayPiece code="wP" ghost /> }}>
      <div className="absolute top-1 right-[13.5%] z-10">
        <div
          data-surface="sheet"
          className="flex flex-col gap-2 rounded-field border border-rule bg-sheet p-2 shadow-lift dark:border-rule-strong"
        >
          <p className="m-0 px-1 text-xs text-ink-soft">Promote to</p>
          <div className="flex gap-1">
            {PIECES.map((p, i) => (
              <span
                key={p.code}
                className={`flex w-14 flex-col items-center gap-1 rounded-field px-1 pt-1 pb-1.5 ${i === 0 ? "bg-well outline-2 outline-offset-0 outline-ink" : ""}`}
              >
                <span className="size-10">
                  <SetPiece code={p.code} set="cburnett" />
                </span>
                <span className="text-2xs text-ink">
                  {p.name} <span className="text-ink-faint">{p.key}</span>
                </span>
              </span>
            ))}
          </div>
        </div>
      </div>
    </Scene>
  );
}

/** C: the board keeps the ghost pawn; the choice is a row of large buttons under it. */
export function PromotionBelow({ px }: { px: number }) {
  return (
    <Scene
      px={px}
      cells={{ h8: <OverlayPiece code="wP" ghost /> }}
      below={
        <div
          data-surface="sheet"
          className="flex flex-col gap-3 rounded-sheet bg-sheet px-4 py-4"
          style={{ width: px }}
        >
          <p className="m-0 text-base font-medium text-ink">Promote the pawn on h8 to</p>
          <div className="grid grid-cols-4 gap-2">
            {PIECES.map((p, i) => (
              <span
                key={p.code}
                className={`flex flex-col items-center gap-1 rounded-field border py-2 ${i === 0 ? "border-ink bg-well" : "border-rule-strong"}`}
              >
                <span className="size-10">
                  <SetPiece code={p.code} set="cburnett" />
                </span>
                <span className={`text-sm text-ink ${i === 0 ? "font-semibold" : ""}`}>
                  {p.name}
                </span>
              </span>
            ))}
          </div>
          <p className="m-0 text-xs text-ink-soft">Escape or Take back returns the pawn to h7.</p>
        </div>
      }
    />
  );
}
