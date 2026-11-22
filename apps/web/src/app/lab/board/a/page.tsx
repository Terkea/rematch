"use client";

import { type Annotation, AnnotationLayer } from "@ci/ui/annotations";
import type { Square } from "@ci/ui/board";
import { games } from "@ci/ui/fixtures";
import { Children, cloneElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { Chessboard, defaultPieces, type PieceRenderObject } from "react-chessboard";
import { LabFrame } from "../../lab-frame";

// Variant a, "Printed diagram": flat squares in a 1px rule-strong frame, coordinates outside in
// Schibsted xs.
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

const files = ["a", "b", "c", "d", "e", "f", "g", "h"];
const ranks = ["8", "7", "6", "5", "4", "3", "2", "1"];

export default function BoardLabA() {
  return (
    <LabFrame
      surface="board"
      variant="a"
      title="Printed diagram"
      rationale="Flat squares inside a 1px rule-strong frame, like a diagram in a printed annotated game. Coordinates live outside the board in Schibsted xs, so no square carries text. The marks are the product pen (Task 12)."
    >
      <main className="px-(--space-gutter) py-8">
        <div className="mx-auto grid w-fit grid-cols-[16px_auto] grid-rows-[auto_16px]">
          <ol
            aria-hidden="true"
            className="grid grid-rows-8 pr-1 text-right font-sans text-xs text-ink-soft"
          >
            {ranks.map((r) => (
              <li key={r} className="flex items-center justify-end">
                {r}
              </li>
            ))}
          </ol>
          <div
            data-lab-board
            data-ply={blunder.ply}
            data-squares="flat"
            data-coords="outside-sans"
            data-stroke="tapered"
            className="relative box-content border border-rule-strong"
            style={{ width: "calc(var(--board-size) - 24px)" }}
          >
            <Chessboard
              options={{
                id: "lab-a",
                position: blunder.fenAfter,
                pieces,
                showNotation: false,
                allowDragging: false,
                allowDrawingArrows: false,
                showAnimations: false,
                lightSquareStyle: { backgroundColor: "var(--color-board-light)" },
                darkSquareStyle: { backgroundColor: "var(--color-board-dark)" },
                squareStyles: {
                  [from]: { boxShadow: "inset 0 0 0 100vmax var(--color-board-lastmove)" },
                  [to]: { boxShadow: "inset 0 0 0 100vmax var(--color-board-lastmove)" },
                },
              }}
            />
            <div data-lab-marks className="pointer-events-none absolute inset-0">
              <AnnotationLayer annotations={marks} orientation="white" animate={false} />
            </div>
          </div>
          <span />
          <ol
            aria-hidden="true"
            className="grid grid-cols-8 pt-0.5 font-sans text-xs text-ink-soft"
          >
            {files.map((f) => (
              <li key={f} className="text-center">
                {f}
              </li>
            ))}
          </ol>
        </div>
        <p className="mt-4 text-center font-serif text-sm text-ink-soft">
          After {Math.ceil(blunder.ply / 2)}. {blunder.san}?? (blunder)
        </p>
      </main>
    </LabFrame>
  );
}
