"use client";

import { Board, NotationText } from "@ci/ui/board";
import { Icon, XIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import { BoardSize } from "./board/static-board";
import { DRILL } from "./fixtures";
import { MockHeader } from "./shell";

// Three phone layouts for the trainer, in the "how sure are you?" step (the most crowded one):
// the move 64. Nb6# has been played and waits for the confidence answer. From lg the same page
// shows its desktop arrangement.

const played = { from: DRILL.right.from, to: DRILL.right.to } as const;

function PlayedBoard({ phonePx, wide }: { phonePx: number; wide?: number }) {
  return (
    <>
      <BoardSize px={phonePx} className="flex justify-center lg:hidden">
        <Board
          fen={DRILL.right.fenAfter}
          label="Your move: 64. Nb6#. How sure are you?"
          orientation="white"
          lastMove={played}
        />
      </BoardSize>
      <div className="hidden lg:block">
        {wide ? (
          <BoardSize px={wide}>
            <Board
              fen={DRILL.right.fenAfter}
              label="Your move: 64. Nb6#. How sure are you?"
              orientation="white"
              lastMove={played}
            />
          </BoardSize>
        ) : (
          <Board
            fen={DRILL.right.fenAfter}
            label="Your move: 64. Nb6#. How sure are you?"
            orientation="white"
            lastMove={played}
          />
        )}
      </div>
    </>
  );
}

const yourMove = <NotationText moveNumber={64} color="white" san="Nb6#" />;

function Confidence({ stacked = false }: { stacked?: boolean }) {
  return (
    <div className={`flex gap-2 ${stacked ? "flex-col" : "flex-wrap"}`}>
      <Button className={stacked ? "w-full" : ""}>Sure</Button>
      <Button variant="outline" className={stacked ? "w-full" : ""}>
        A guess
      </Button>
      <Button variant="quiet" className={stacked ? "w-full" : ""}>
        Take back
      </Button>
    </div>
  );
}

function Strip({ now = 3 }: { now?: number }) {
  return (
    <div
      role="img"
      className="m-0 flex items-center gap-1"
      aria-label="Position 3 of 8; 2 right so far"
    >
      {Array.from({ length: 8 }, (_, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed row
          key={i}
          className={`h-1.5 flex-1 rounded-pill ${i < now - 1 ? "bg-good" : i === now - 1 ? "bg-ink" : "bg-rule"}`}
        />
      ))}
    </div>
  );
}

/** A: the app header, the board in the gutter, everything else under it in reading order. */
export function PhoneA() {
  return (
    <>
      <MockHeader sound="header" />
      <div className="mx-auto flex max-w-6xl flex-col gap-4 px-(--space-gutter) py-5 lg:py-8">
        <div className="flex flex-col gap-1">
          <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
            Position 3 of 8, 2 right so far
          </p>
          <h1 className="m-0 font-serif text-2xl font-semibold text-ink">Find the best move</h1>
        </div>
        <div className="flex flex-col gap-5 lg:flex-row lg:gap-8">
          <PlayedBoard phonePx={352} />
          <div className="flex max-w-md flex-col gap-3">
            <p className="m-0 text-base text-ink">Your move: {yourMove}. How sure are you?</p>
            <Confidence />
          </div>
        </div>
      </div>
    </>
  );
}

/** B: edge to edge. The board takes the full phone width; the prompt sits above, the answer in a thumb bar. */
export function PhoneB() {
  return (
    <>
      <MockHeader sound="header" />
      <div className="mx-auto flex max-w-6xl flex-col gap-3 py-4 lg:px-(--space-gutter) lg:py-8">
        <div className="flex items-baseline justify-between gap-3 px-(--space-gutter) lg:px-0">
          <h1 className="m-0 font-serif text-xl font-semibold text-ink lg:text-2xl">
            White to move
          </h1>
          <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">3 of 8</p>
        </div>
        <div className="flex flex-col gap-4 lg:flex-row lg:gap-8">
          <div className="flex flex-col gap-2">
            <PlayedBoard phonePx={384} wide={560} />
            <div className="px-(--space-gutter) lg:px-0">
              <Strip />
            </div>
          </div>
          <div className="flex flex-col gap-3 px-(--space-gutter) max-lg:sticky max-lg:bottom-0 max-lg:border-t max-lg:border-rule max-lg:bg-paper max-lg:py-3 lg:max-w-md lg:px-0">
            <p className="m-0 text-base text-ink">Your move: {yourMove}. How sure are you?</p>
            <Confidence />
          </div>
        </div>
      </div>
    </>
  );
}

/** C: a focus mode. No app nav while training: an exit, the count, then the board and big answers. */
export function PhoneC() {
  return (
    <div className="mx-auto flex min-h-dvh max-w-6xl flex-col">
      <header className="flex h-14 items-center gap-3 px-(--space-gutter)">
        <Button variant="quiet" size="sm" className="-ml-2">
          <Icon icon={XIcon} />
          Stop for today
        </Button>
        <div className="ml-auto w-32">
          <Strip />
        </div>
        <span className="text-sm text-ink-soft [font-variant-numeric:lining-nums]">3 of 8</span>
      </header>
      <div className="flex flex-1 flex-col gap-4 px-(--space-gutter) pb-5 lg:flex-row lg:gap-10 lg:pt-6">
        <PlayedBoard phonePx={352} wide={560} />
        <div className="flex flex-1 flex-col gap-4 lg:max-w-sm">
          <div className="flex flex-col gap-1">
            <h1 className="m-0 font-serif text-2xl font-semibold text-ink">Find the best move</h1>
            <p className="m-0 text-base text-ink">Your move: {yourMove}. How sure are you?</p>
          </div>
          <div className="mt-auto lg:mt-0">
            <Confidence stacked />
          </div>
        </div>
      </div>
    </div>
  );
}
