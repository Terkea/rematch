"use client";

import { Board } from "@ci/ui/board";
import type { ReactNode } from "react";
import { DRILL } from "./fixtures";

/** The trainer's first frame, as the backdrop for the sound-control placements. */
export function TrainerScene({ belowBoard, aside }: { belowBoard?: ReactNode; aside?: ReactNode }) {
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 px-(--space-gutter) py-8">
      <div className="flex flex-col gap-1">
        <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
          Position 3 of 8, 2 of 2 right so far
        </p>
        <h1 className="m-0 font-serif text-2xl font-semibold text-ink">Find the best move</h1>
      </div>
      <div className="flex flex-col gap-8 lg:flex-row lg:items-start">
        <div className="flex flex-col gap-3">
          <Board
            fen={DRILL.fen}
            label="Position after 63… Ka4. Find the best move."
            orientation="white"
            lastMove={DRILL.lastMove}
          />
          {belowBoard}
        </div>
        <div className="flex min-w-0 max-w-md flex-1 flex-col gap-4">
          <p className="m-0 text-base text-ink">White to move.</p>
          <p className="m-0 text-base text-ink-soft">
            Play the move you would choose. Drag a piece, or pick it with the arrow keys and Enter.
          </p>
          {aside}
        </div>
      </div>
    </div>
  );
}
