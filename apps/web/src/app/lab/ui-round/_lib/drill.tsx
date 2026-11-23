"use client";

import type { Annotation } from "@ci/ui/annotations";
import { Board, NotationText } from "@ci/ui/board";
import { MarginNote } from "@ci/ui/brand";
import { CheckCircleIcon, Icon, SpeakerHighIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import type { ReactNode } from "react";
import { BoardOverlay, BoardSize, MotionTrace, OverlayPiece } from "./board/static-board";
import { DRILL } from "./fixtures";

// The trainer's drill feedback as storyboards: each frame is a still of the real Board at one
// moment of the choreography (DESIGN.md "Choreography": drill right and drill wrong), with the
// time, what moves and the sound under it. Frames use the shipped components and tokens only.

type Sq = Parameters<typeof Board>[0]["lastMove"] extends infer T
  ? T extends { from: infer F }
    ? F
    : never
  : never;

const sq = (s: string) => s as Sq;
const W = DRILL.wrong;
const R = DRILL.right;
const N = DRILL.moveNumber;

export interface FrameSpec {
  at: string;
  title: string;
  motion: string;
  sound?: string;
  fen: string;
  last?: [string, string];
  marks?: Annotation[];
  ghost?: { square: string; code: string };
  trace?: [string, string];
  panel?: ReactNode;
}

export function Frame({ spec, index, px = 288 }: { spec: FrameSpec; index: number; px?: number }) {
  return (
    <figure className="m-0 flex min-w-0 flex-col gap-3">
      <BoardSize px={px} className="relative w-fit">
        <div className="relative">
          <Board
            fen={spec.fen}
            label={`Frame ${index}: ${spec.title}`}
            orientation="white"
            {...(spec.last ? { lastMove: { from: sq(spec.last[0]), to: sq(spec.last[1]) } } : {})}
            {...(spec.marks ? { annotations: spec.marks } : {})}
            animateAnnotations={false}
            className="md:bg-transparent md:p-0"
          />
          {spec.ghost || spec.trace ? (
            <BoardOverlay
              cells={
                spec.ghost
                  ? { [spec.ghost.square]: <OverlayPiece code={spec.ghost.code} ghost /> }
                  : {}
              }
            >
              {spec.trace ? <MotionTrace from={spec.trace[0]} to={spec.trace[1]} /> : null}
            </BoardOverlay>
          ) : null}
        </div>
      </BoardSize>
      <figcaption className="flex max-w-72 flex-col gap-1 text-sm">
        <span className="text-ink [font-variant-numeric:lining-nums]">
          <span className="font-semibold">{index}.</span> {spec.title}{" "}
          <span className="text-ink-faint">{spec.at}</span>
        </span>
        <span className="text-ink-soft">{spec.motion}</span>
        {spec.sound ? (
          <span className="flex items-center gap-1.5 text-ink-soft">
            <Icon icon={SpeakerHighIcon} className="shrink-0" />
            {spec.sound}
          </span>
        ) : null}
      </figcaption>
      {spec.panel ? (
        <div
          data-surface="sheet"
          className="flex max-w-72 flex-col gap-2 rounded-sheet bg-sheet px-4 py-3"
        >
          {spec.panel}
        </div>
      ) : null}
    </figure>
  );
}

export function Sequence({
  title,
  lede,
  frames,
}: {
  title: string;
  lede: string;
  frames: FrameSpec[];
}) {
  return (
    <section className="flex flex-col gap-5">
      <div className="flex max-w-prose flex-col gap-1">
        <h2 className="m-0 font-serif text-2xl font-semibold text-ink">{title}</h2>
        <p className="m-0 text-base text-ink-soft">{lede}</p>
      </div>
      <ol className="m-0 grid list-none grid-cols-1 gap-x-6 gap-y-8 p-0 md:grid-cols-2 xl:grid-cols-4">
        {frames.map((f, i) => (
          <li key={f.title}>
            <Frame spec={f} index={i + 1} />
          </li>
        ))}
      </ol>
    </section>
  );
}

const you = (san: string) => <NotationText moveNumber={N} color="white" san={san} />;
const penMark: Annotation = { kind: "mark", square: sq(W.to), glyph: "?", tone: "pen" };
const goodRing: Annotation = { kind: "circle", square: sq(R.to), tone: "good" };
const goodMark: Annotation = { kind: "mark", square: sq(R.to), glyph: "!", tone: "good" };

function Streak({ n, broken = false }: { n: number; broken?: boolean }) {
  return (
    <p className="m-0 flex items-baseline gap-2 text-sm text-ink-soft [font-variant-numeric:lining-nums_tabular-nums]">
      <span className="font-serif text-2xl font-semibold text-ink">{n}</span>
      {broken ? "in a row before this one" : "right in a row"}
    </p>
  );
}

function Verdict({ good, children }: { good?: boolean; children: ReactNode }) {
  return (
    <p
      className={`m-0 flex items-start gap-2 font-serif text-lg font-semibold ${good ? "text-good-text" : "text-ink"}`}
    >
      {good ? <Icon icon={CheckCircleIcon} className="mt-1 shrink-0" /> : null}
      <span>{children}</span>
    </p>
  );
}

/** Session marks: one small square per position, the scoresheet's result mark. */
function SessionMarks({ marks }: { marks: ("right" | "wrong" | "now" | "todo")[] }) {
  return (
    <div
      role="img"
      className="m-0 flex flex-wrap items-center gap-1.5"
      aria-label="This session so far"
    >
      {marks.map((m, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed storyboard row
          key={i}
          className={`flex size-6 items-center justify-center rounded-board border text-xs font-semibold ${
            m === "right"
              ? "border-good bg-sheet text-good-text"
              : m === "wrong"
                ? "border-pen bg-sheet text-pen-text"
                : m === "now"
                  ? "border-ink bg-well text-ink"
                  : "border-rule bg-paper text-ink-faint"
          }`}
        >
          {m === "right" ? "!" : m === "wrong" ? "?" : i + 1}
        </span>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- A: spring back, one retry

export const A_WRONG: FrameSpec[] = [
  {
    at: "0 ms",
    title: "The queen lands on c3",
    motion: "Lift, travel on the queen's settle (175 ms to 90%), land with its squash.",
    sound: "Move, on the contact frame",
    fen: W.fenAfter,
    last: [W.from, W.to],
  },
  {
    at: "+80 to 255 ms",
    title: "It springs back to d3",
    motion:
      "After an 80 ms beat the queen travels back on settle, with no landing squash. The board never holds the wrong move.",
    sound: "Wrong: the muted thud, as it leaves c3",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    ghost: { square: W.to, code: W.piece },
    trace: [W.to, W.from],
  },
  {
    at: "255 to 1,155 ms",
    title: "A pen ? lands on c3",
    motion: "The badge lands on the land spring and stays 900 ms, then fades over 160 ms.",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    marks: [penMark],
    panel: (
      <>
        <Verdict>Not that one. Have another look.</Verdict>
        <p className="m-0 text-sm text-ink-soft">
          You played {you(W.san)}. One more try, then the answer.
        </p>
      </>
    ),
  },
  {
    at: "1,155 ms on",
    title: "Second try, hint on offer",
    motion:
      "The board is live again. A hint draws one pencil ring on the piece to move; it never names the square.",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    marks: [{ kind: "circle", square: sq(R.from), tone: "pencil" }],
    panel: (
      <>
        <p className="m-0 text-sm text-ink">Second try. The hint: this piece.</p>
        <Streak n={2} broken />
        <Button variant="quiet" size="sm" className="w-fit">
          Show the answer
        </Button>
      </>
    ),
  },
];

export const A_RIGHT: FrameSpec[] = [
  {
    at: "0 ms",
    title: "The knight lands on b6",
    motion: "The knight arcs over d5 to b6 (154 ms to 90%) and lands with its squash.",
    sound: "Move on contact; no check tick, mate takes the correct sound",
    fen: R.fenAfter,
    last: [R.from, R.to],
  },
  {
    at: "0 to 420 ms",
    title: "A good ring draws",
    motion: "The ring starts at one o'clock and overruns to 380°, on ease-pen.",
    sound: "Correct, with the ring",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing],
  },
  {
    at: "420 ms",
    title: "The ! lands, the streak ticks to 3",
    motion:
      "Badge and streak number both land on the land spring; the number rolls with NumberFlow.",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <Verdict good>There you go: {you(R.san)} is mate.</Verdict>
        <Streak n={3} />
      </>
    ),
  },
  {
    at: "600 ms on",
    title: "The margin note, then Next",
    motion:
      "The crayon stroke draws under dad's note once; Next takes focus. Nothing advances by itself.",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <MarginNote celebrate>there you go</MarginNote>
        <Button size="sm" className="w-fit">
          Next position
        </Button>
      </>
    ),
  },
];

// ---------------------------------------------------------------- B: one attempt, show at once

export const B_WRONG: FrameSpec[] = [
  { ...(A_WRONG[0] as FrameSpec) },
  {
    ...(A_WRONG[1] as FrameSpec),
    motion: "The queen springs straight back on settle. No retry: the answer follows at once.",
  },
  {
    at: "255 to 795 ms",
    title: "Your move in pen, the answer in green",
    motion:
      "The pen arrow d3–c3 and its ? draw first; 120 ms later the good arrow d5–b6 and its !.",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    marks: [
      { kind: "arrow", from: sq(W.from), to: sq(W.to), tone: "pen", toEdge: true },
      penMark,
      { kind: "arrow", from: sq(R.from), to: sq(R.to), tone: "good" },
      goodMark,
    ],
    panel: (
      <>
        <Verdict>Not quite. {you(R.san)} was mate in one.</Verdict>
        <p className="m-0 text-sm text-ink-soft">You played {you(W.san)}, which mates in three.</p>
      </>
    ),
  },
  {
    at: "800 ms on",
    title: "It comes back tomorrow",
    motion: "The session strip writes a ? for this position; the next due day is said in words.",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    marks: [
      { kind: "arrow", from: sq(W.from), to: sq(W.to), tone: "pen", toEdge: true },
      penMark,
      { kind: "arrow", from: sq(R.from), to: sq(R.to), tone: "good" },
      goodMark,
    ],
    panel: (
      <>
        <SessionMarks marks={["right", "right", "wrong", "todo", "todo", "todo", "todo", "todo"]} />
        <p className="m-0 text-sm text-ink-soft">This position comes back tomorrow.</p>
        <Button size="sm" className="w-fit">
          Next position
        </Button>
      </>
    ),
  },
];

export const B_RIGHT: FrameSpec[] = [
  { ...(A_RIGHT[0] as FrameSpec) },
  {
    ...(A_RIGHT[2] as FrameSpec),
    at: "0 to 540 ms",
    title: "Ring and ! together",
    motion: "The good ring draws (420 ms) and the ! lands as it closes; no separate streak number.",
    sound: "Correct, with the ring",
    panel: undefined,
  },
  {
    at: "540 ms",
    title: "The strip writes a !",
    motion: "The third square of the session strip lands on the land spring.",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <Verdict good>There you go: {you(R.san)} is mate.</Verdict>
        <SessionMarks marks={["right", "right", "right", "now", "todo", "todo", "todo", "todo"]} />
      </>
    ),
  },
  {
    at: "on Next",
    title: "The next position turns in",
    motion: "The board cross-fades to position 4 over 160 ms; the strip's current square moves on.",
    fen: "r5k1/2p4p/1p1p2p1/1B1P4/2P1b3/1KQ5/5q2/1R4R1 w - - 0 36",
    last: ["f4", "f2"],
    panel: (
      <>
        <p className="m-0 font-serif text-lg font-semibold text-ink">Find the best move</p>
        <SessionMarks marks={["right", "right", "right", "now", "todo", "todo", "todo", "todo"]} />
      </>
    ),
  },
];

// ---------------------------------------------------------------- C: the session scoresheet

function SheetRows({
  rows,
}: {
  rows: { n: number; text: ReactNode; tone?: "pen" | "good" | "ink" }[];
}) {
  return (
    <ol className="m-0 flex list-none flex-col p-0">
      {rows.map((r) => (
        <li
          key={r.n}
          className="flex h-8 items-center gap-3 border-b border-rule text-sm [font-variant-numeric:lining-nums]"
        >
          <span className="w-4 text-ink-faint">{r.n}</span>
          <span
            className={
              r.tone === "pen" ? "text-pen-text" : r.tone === "good" ? "text-good-text" : "text-ink"
            }
          >
            {r.text}
          </span>
        </li>
      ))}
    </ol>
  );
}

const DONE = [
  { n: 1, text: "Found, first try", tone: "good" as const },
  { n: 2, text: "Found, first try", tone: "good" as const },
];

export const C_WRONG: FrameSpec[] = [
  { ...(A_WRONG[0] as FrameSpec) },
  { ...(A_WRONG[1] as FrameSpec) },
  {
    at: "255 to 600 ms",
    title: "The sheet writes 64. Qc3?",
    motion:
      "A pen ring draws on c3 as the line writes itself into the session sheet (stagger-tight).",
    fen: DRILL.fen,
    last: [DRILL.lastMove.from, DRILL.lastMove.to],
    marks: [{ kind: "circle", square: sq(W.to), tone: "pen" }, penMark],
    panel: (
      <>
        <p className="m-0 text-sm text-ink-soft">Today's sheet, streak 2</p>
        <SheetRows rows={[...DONE, { n: 3, text: "64. Qc3?", tone: "pen" }]} />
        <p className="m-0 text-sm text-ink">Try again: 1 try left.</p>
      </>
    ),
  },
  {
    at: "second try",
    title: "Found on the second try",
    motion:
      "The knight lands; the sheet adds 64. Nb6#! under the first try. A second-try solve counts half for the streak.",
    sound: "Correct, quieter (80%) for a second try",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <p className="m-0 text-sm text-ink-soft">Today's sheet, streak 2</p>
        <SheetRows rows={[...DONE, { n: 3, text: "64. Qc3?  then  64. Nb6#!", tone: "ink" }]} />
      </>
    ),
  },
];

export const C_RIGHT: FrameSpec[] = [
  { ...(A_RIGHT[0] as FrameSpec) },
  { ...(A_RIGHT[1] as FrameSpec) },
  {
    at: "420 ms",
    title: "The ! lands and the sheet writes it",
    motion: "64. Nb6#! writes into row 3 in green; the streak in the sheet's head ticks to 3.",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <p className="m-0 text-sm text-ink-soft">Today's sheet, streak 3</p>
        <SheetRows rows={[...DONE, { n: 3, text: "64. Nb6#!", tone: "good" }]} />
      </>
    ),
  },
  {
    at: "end of session",
    title: "The finished sheet",
    motion: "After position 8 the sheet is the summary: what you found, what comes back and when.",
    fen: R.fenAfter,
    last: [R.from, R.to],
    marks: [goodRing, goodMark],
    panel: (
      <>
        <MarginNote celebrate>a good session</MarginNote>
        <p className="m-0 text-sm text-ink-soft">6 of 8 found. 2 positions come back tomorrow.</p>
      </>
    ),
  },
];
