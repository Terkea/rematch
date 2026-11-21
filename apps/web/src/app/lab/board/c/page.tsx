"use client";

import { type Annotation, AnnotationLayer } from "@ci/ui/annotations";
import type { Square } from "@ci/ui/board";
import { games } from "@ci/ui/fixtures";
import { Children, cloneElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { Chessboard, defaultPieces, type PieceRenderObject } from "react-chessboard";
import { LabFrame } from "../../lab-frame";

// Variant c, "Ink well": the board pressed into a well, coordinates on the squares, and a
// variable-width pen stroke with tapered ends (DESIGN.md, "Pen stroke geometry").
// Lab only: Task 11 builds the real wrapper and packages/ui/src/annotations the real geometry.

const game = games.middlegameBlunder;
const found = game.plies.find((p) => p.judgement === "blunder");
if (!found) throw new Error("middlegameBlunder has no blunder");
const blunder = found;
const from = blunder.uci.slice(0, 2) as Square;
const to = blunder.uci.slice(2, 4) as Square;

// The marks are the product's red pen (`AnnotationLayer`, Task 12): this lab compares the
// board treatments (squares, frame, coordinates); the stroke comparison is recorded in
// packages/ui/design/decisions/board.md.
const marks: Annotation[] = [
  { kind: "circle", square: to, tone: "pen" },
  { kind: "arrow", from, to, tone: "pen" },
  { kind: "mark", square: to, glyph: "??", tone: "pen" },
];

// Cburnett paths (react-chessboard's default set, identical to the Commons files) recoloured to
// the piece tokens. Lab only: the product recolours at build time into sprites (DESIGN.md).
function recolour(node: ReactNode): ReactNode {
  if (!isValidElement(node)) return node;
  const el = node as ReactElement<{ style?: Record<string, unknown>; children?: ReactNode }>;
  const map = (v: unknown) =>
    v === "#000000" ? "var(--color-piece-dark)" : v === "#ffffff" ? "var(--color-piece-light)" : v;
  const style = el.props.style
    ? Object.fromEntries(Object.entries(el.props.style).map(([k, v]) => [k, map(v)]))
    : undefined;
  const kids = el.props.children;
  return cloneElement(el, {
    ...(style ? { style } : {}),
    ...(kids === undefined
      ? {}
      : { children: Array.isArray(kids) ? Children.toArray(kids).map(recolour) : recolour(kids) }),
  });
}

const pieces: PieceRenderObject = Object.fromEntries(
  Object.entries(defaultPieces).map(([key, render]) => [
    key,
    (props?: Parameters<typeof render>[0]) => recolour(render(props)) as ReactElement,
  ]),
);

const coord = {
  fontFamily: "var(--font-sans)",
  fontSize: "var(--text-xs)",
  lineHeight: 1,
  fontWeight: 500,
  position: "absolute" as const,
  userSelect: "none" as const,
};

export default function BoardLabC() {
  return (
    <LabFrame
      surface="board"
      variant="c"
      title="Ink well"
      rationale="The board is pressed into a well with --radius-board, so it reads as set into the sheet rather than framed. Coordinates sit on the squares (no outside lane, so the board keeps its pixels on a phone). The marks are the product pen (Task 12)."
    >
      <main className="px-(--space-gutter) py-8">
        <div className="mx-auto w-fit rounded-(--radius-board) bg-well p-2">
          <div
            data-lab-board
            data-ply={blunder.ply}
            data-squares="well"
            data-coords="on-squares"
            data-stroke="tapered"
            className="relative overflow-hidden rounded-(--radius-board) [--lab-coord-light:var(--color-ink)] dark:[--lab-coord-light:var(--color-on-ink)]"
            style={{ width: "calc(var(--board-size) - 16px)" }}
          >
            <Chessboard
              options={{
                id: "lab-c",
                position: blunder.fenAfter,
                pieces,
                allowDragging: false,
                allowDrawingArrows: false,
                showAnimations: false,
                boardStyle: { borderRadius: "var(--radius-board)" },
                lightSquareStyle: { backgroundColor: "var(--color-board-light)" },
                darkSquareStyle: { backgroundColor: "var(--color-board-dark)" },
                lightSquareNotationStyle: { color: "var(--lab-coord-light)" },
                darkSquareNotationStyle: { color: "var(--color-ink)" },
                alphaNotationStyle: { ...coord, bottom: 2, right: 3 },
                numericNotationStyle: { ...coord, top: 3, left: 3 },
                squareStyles: {
                  [from]: { backgroundImage: "linear-gradient(var(--color-board-lastmove) 0 0)" },
                  [to]: { backgroundImage: "linear-gradient(var(--color-board-lastmove) 0 0)" },
                },
              }}
            />
            <div data-lab-marks className="pointer-events-none absolute inset-0">
              <AnnotationLayer annotations={marks} orientation="white" animate={false} />
            </div>
          </div>
        </div>
        <p className="mt-4 text-center font-serif text-sm text-ink-soft">
          After {Math.ceil(blunder.ply / 2)}. {blunder.san}?? (blunder)
        </p>
      </main>
    </LabFrame>
  );
}
