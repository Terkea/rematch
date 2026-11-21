"use client";

import { type KeyboardEvent, useId, useRef } from "react";
import { COORDINATE, cellToSquare, type Orientation, type Square, squareToCell } from "./theme.js";

// The coordinate glyph boxes as CSS lengths on a square (theme.ts COORDINATE), plus 2px of clear
// space, so a capture ring or focus ring is cut where it would run over a rank number or file letter.
const GLYPH_W = `calc(${COORDINATE.inset + 2}px + ${COORDINATE.size * COORDINATE.advance * 100}%)`;
const GLYPH_H = `calc(${COORDINATE.inset + 2}px + ${COORDINATE.size * 100}%)`;

export interface SquareGridProps {
  label: string;
  orientation: Orientation;
  /** "e4, white knight" for each square: the piece names by square */
  names: ReadonlyMap<Square, string>;
  /** the roving tab stop */
  focus: Square;
  onFocusChange: (square: Square) => void;
  selected: Square | null;
  /** legal destinations of the selected piece: true when the move captures */
  targets: ReadonlyMap<Square, boolean>;
  check?: Square | undefined;
  onActivate: (square: Square) => void;
  onCancel: () => void;
}

const ARROWS: Record<string, [number, number]> = {
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
};

/**
 * Keyboard access to the board: an 8 × 8 grid of transparent buttons over it, with
 * `pointer-events: none` so the mouse reaches the board underneath. It has a roving tabindex: the
 * arrow keys move focus as the squares are drawn, Enter or Space picks up and drops, and Escape
 * cancels. Focus is the dashed inset ring on a `sheet` halo, inside the check ring when both apply
 * (DESIGN.md, "Focus"). Legal moves are `ink` dots and capture rings, each on a 2px halo.
 */
export function SquareGrid({
  label,
  orientation,
  names,
  focus,
  onFocusChange,
  selected,
  targets,
  check,
  onActivate,
  onCancel,
}: SquareGridProps) {
  const buttons = useRef(new Map<Square, HTMLButtonElement>());
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = ARROWS[event.key];
    if (step) {
      event.preventDefault();
      const { col, row } = squareToCell(focus, orientation);
      const next = cellToSquare(
        Math.min(7, Math.max(0, col + step[0])),
        Math.min(7, Math.max(0, row + step[1])),
        orientation,
      );
      onFocusChange(next);
      buttons.current.get(next)?.focus();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onActivate(focus);
      return;
    }
    if (event.key === "Escape") onCancel();
  };

  const cells: Square[] = [];
  for (let row = 0; row < 8; row++) {
    for (let col = 0; col < 8; col++) cells.push(cellToSquare(col, row, orientation));
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of 64 toggle buttons, not a fieldset
    <div
      role="group"
      aria-label={label}
      onKeyDown={onKeyDown}
      className="pointer-events-none absolute inset-0 z-20 grid grid-cols-8 grid-rows-8"
    >
      {cells.map((square) => {
        const capture = targets.get(square);
        const inset = square === check ? 7 : 2;
        // Ranks are drawn on the left column, files on the bottom row (as seen).
        const { col, row } = squareToCell(square, orientation);
        const cut = col === 0 || row === 7 ? `${uid}-cut-${square}` : undefined;
        const masked = cut ? `url(#${cut})` : undefined;
        return (
          <button
            key={square}
            ref={(el) => {
              if (el) buttons.current.set(square, el);
              else buttons.current.delete(square);
            }}
            type="button"
            data-square-button={square}
            tabIndex={square === focus ? 0 : -1}
            aria-label={names.get(square) ?? `${square}, empty`}
            aria-pressed={square === selected ? true : undefined}
            onFocus={() => {
              if (square !== focus) onFocusChange(square);
            }}
            className="group relative appearance-none bg-transparent p-0 outline-none"
          >
            <svg aria-hidden="true" className="absolute inset-0 size-full overflow-visible">
              {cut ? (
                <mask id={cut} maskUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
                  <rect x="0" y="0" width="100%" height="100%" fill="white" />
                  {col === 0 ? (
                    <rect
                      data-cut="rank"
                      x="0"
                      y="0"
                      fill="black"
                      style={{ width: GLYPH_W, height: GLYPH_H }}
                    />
                  ) : null}
                  {row === 7 ? (
                    <rect
                      data-cut="file"
                      fill="black"
                      style={{
                        x: `calc(100% - ${GLYPH_W})`,
                        y: `calc(100% - ${GLYPH_H})`,
                        width: GLYPH_W,
                        height: GLYPH_H,
                      }}
                    />
                  ) : null}
                </mask>
              ) : null}
              {capture === false ? (
                <>
                  <circle
                    cx="50%"
                    cy="50%"
                    r="12%"
                    fill="var(--color-sheet)"
                    stroke="var(--color-sheet)"
                    strokeWidth={4}
                  />
                  <circle cx="50%" cy="50%" r="12%" fill="var(--color-ink)" />
                </>
              ) : null}
              {capture === true ? (
                <g mask={masked} data-capture-ring>
                  <circle
                    cx="50%"
                    cy="50%"
                    r="42%"
                    fill="none"
                    stroke="var(--color-sheet)"
                    style={{ strokeWidth: "calc(8% + 4px)" }}
                  />
                  <circle
                    cx="50%"
                    cy="50%"
                    r="42%"
                    fill="none"
                    stroke="var(--color-ink)"
                    strokeWidth="8%"
                  />
                </g>
              ) : null}
              <g className="hidden group-focus-visible:inline" mask={masked}>
                <rect
                  x={inset + 1}
                  y={inset + 1}
                  fill="none"
                  stroke="var(--color-sheet)"
                  strokeWidth={2}
                  style={{
                    width: `calc(100% - ${2 * (inset + 1)}px)`,
                    height: `calc(100% - ${2 * (inset + 1)}px)`,
                  }}
                />
                <rect
                  x={inset + 3}
                  y={inset + 3}
                  fill="none"
                  stroke="var(--color-ink)"
                  strokeWidth={2}
                  strokeDasharray="4 3"
                  style={{
                    width: `calc(100% - ${2 * (inset + 3)}px)`,
                    height: `calc(100% - ${2 * (inset + 3)}px)`,
                  }}
                />
              </g>
            </svg>
          </button>
        );
      })}
    </div>
  );
}
