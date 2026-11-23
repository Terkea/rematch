import { PieceGlyph } from "@ci/ui/board";
import type { CSSProperties, ReactNode } from "react";
import { chessnut } from "./chessnut";

// Lab-only board drawing for the ui-round mockups: board colourings scoped by a class, a static
// 8x8 board that can draw either piece set, and an overlay grid aligned to a real `Board`, for
// storyboard frames (a ghost piece, a motion trace) and the promotion pickers. Board geometry may
// use raw values here (board/**), colours never: every colouring is a mix of existing tokens.

export type BoardColouring = "slate" | "paper" | "pencil";
export type PieceSetName = "cburnett" | "chessnut";

export const COLOURINGS: Record<
  BoardColouring,
  { name: string; note: string; day: string; night: string }
> = {
  slate: {
    name: "Slate",
    note: "The shipped squares: board-light and board-dark.",
    day: "board-light, board-dark",
    night: "board-light, board-dark",
  },
  paper: {
    name: "Paper",
    note: "Lighter and quieter: the sheet against a rule mix. Pieces and marks carry more of the eye.",
    day: "sheet; rule + 25% rule-strong",
    night: "ink-faint + 20% ink; rule-strong + 30% ink-faint",
  },
  pencil: {
    name: "Pencil",
    note: "More contrast between squares, closest to a printed diagram: well against a pencil mix.",
    day: "well; pencil + 35% rule",
    night: "pencil + 30% ink-soft; rule-strong + 25% rule",
  },
};

/** Scoped overrides of the two square tokens; slate keeps the shipped values. */
export const COLOURING_CSS = `
.uir-board-paper{--color-board-light:var(--color-sheet);--color-board-dark:color-mix(in srgb,var(--color-rule),var(--color-rule-strong) 25%)}
[data-theme="dark"] .uir-board-paper{--color-board-light:color-mix(in srgb,var(--color-ink-faint),var(--color-ink) 20%);--color-board-dark:color-mix(in srgb,var(--color-rule-strong),var(--color-ink-faint) 30%)}
.uir-board-pencil{--color-board-light:var(--color-well);--color-board-dark:color-mix(in srgb,var(--color-pencil),var(--color-rule) 35%)}
[data-theme="dark"] .uir-board-pencil{--color-board-light:color-mix(in srgb,var(--color-pencil),var(--color-ink-soft) 30%);--color-board-dark:color-mix(in srgb,var(--color-rule-strong),var(--color-rule) 25%)}
`;

export function ColouringStyles() {
  return <style data-uir-colourings>{COLOURING_CSS}</style>;
}

export function colouringClass(c: BoardColouring): string {
  return c === "slate" ? "" : `uir-board-${c}`;
}

/** A sized scope: sets `--board-size` for the real Board inside (frames, phone mockups). */
export function BoardSize({
  px,
  children,
  className,
}: {
  px: number;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={className} style={{ "--board-size": `${px}px` } as CSSProperties}>
      {children}
    </div>
  );
}

const FILES = "abcdefgh";

/** Square (as a1..h8) to its column and row as drawn. */
export function cell(square: string, orientation: "white" | "black" = "white") {
  const file = FILES.indexOf(square[0] ?? "a");
  const rank = Number(square[1]) - 1;
  return orientation === "white" ? { col: file, row: 7 - rank } : { col: 7 - file, row: rank };
}

export function Chessnut({ code }: { code: string }) {
  return (
    <svg
      viewBox="0 0 800 800"
      width="100%"
      height="100%"
      aria-hidden="true"
      focusable="false"
      style={{ display: "block", fill: "var(--color-piece-dark)", overflow: "visible" }}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: lab-only, generated from the committed set
      dangerouslySetInnerHTML={{ __html: chessnut[code] ?? "" }}
    />
  );
}

export function SetPiece({ code, set }: { code: string; set: PieceSetName }) {
  return set === "chessnut" ? (
    <Chessnut code={code} />
  ) : (
    <PieceGlyph code={code as Parameters<typeof PieceGlyph>[0]["code"]} />
  );
}

function placement(fen: string): Map<string, string> {
  const map = new Map<string, string>();
  (fen.split(" ")[0] ?? "").split("/").forEach((row, i) => {
    let f = 0;
    for (const ch of row) {
      if (/\d/.test(ch)) f += Number(ch);
      else {
        map.set(`${FILES[f]}${8 - i}`, `${ch === ch.toUpperCase() ? "w" : "b"}${ch.toUpperCase()}`);
        f += 1;
      }
    }
  });
  return map;
}

/**
 * A static board in the shipped treatment (flat squares, coordinates on the squares, pieces at
 * 0.84 of the square), for comparing colourings and piece sets side by side. Not interactive.
 */
export function StaticBoard({
  fen,
  set = "cburnett",
  colouring = "slate",
  px,
  lastMove,
  label,
}: {
  fen: string;
  set?: PieceSetName;
  colouring?: BoardColouring;
  px: number;
  lastMove?: [string, string];
  label: string;
}) {
  const pieces = placement(fen);
  const sq = px / 8;
  const coord: CSSProperties = {
    position: "absolute",
    fontFamily: "var(--font-sans)",
    fontWeight: 500,
    fontSize: sq * 0.2,
    lineHeight: 1,
  };
  return (
    <div
      role="img"
      aria-label={label}
      className={colouringClass(colouring)}
      style={{
        width: px,
        height: px,
        display: "grid",
        gridTemplateColumns: "repeat(8, 1fr)",
        borderRadius: "var(--radius-board)",
        overflow: "hidden",
      }}
    >
      {Array.from({ length: 64 }, (_, i) => {
        const row = Math.floor(i / 8);
        const col = i % 8;
        const square = `${FILES[col]}${8 - row}`;
        const light = (row + col) % 2 === 0;
        const code = pieces.get(square);
        const tinted = lastMove?.includes(square);
        const coordColour = light ? "var(--board-coord-light)" : "var(--board-coord-dark)";
        return (
          <div
            key={square}
            className="[--board-coord-dark:var(--color-ink)] [--board-coord-light:var(--color-ink)] dark:[--board-coord-light:var(--color-on-ink)]"
            style={{
              position: "relative",
              backgroundColor: light ? "var(--color-board-light)" : "var(--color-board-dark)",
              backgroundImage: tinted
                ? "linear-gradient(var(--color-board-lastmove), var(--color-board-lastmove))"
                : undefined,
            }}
          >
            {col === 0 ? (
              <span style={{ ...coord, top: 2, left: 2, color: coordColour }}>{8 - row}</span>
            ) : null}
            {row === 7 ? (
              <span style={{ ...coord, bottom: 2, right: 2, color: coordColour }}>
                {FILES[col]}
              </span>
            ) : null}
            {code ? (
              <div style={{ position: "absolute", inset: `${sq * 0.08}px` }}>
                <SetPiece code={code} set={set} />
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

/** A mini row for the Settings picker: king, queen, knight and pawn of both colours. */
export function PieceRow({
  set,
  colouring = "slate",
  sq = 36,
}: {
  set: PieceSetName;
  colouring?: BoardColouring;
  sq?: number;
}) {
  const codes = ["wK", "wQ", "wN", "wP", "bK", "bQ", "bN", "bP"];
  return (
    <div
      aria-hidden="true"
      className={colouringClass(colouring)}
      style={{
        display: "flex",
        borderRadius: "var(--radius-board)",
        overflow: "hidden",
        width: "fit-content",
      }}
    >
      {codes.map((c, i) => (
        <div
          key={c}
          style={{
            width: sq,
            height: sq,
            padding: sq * 0.08,
            backgroundColor: i % 2 === 0 ? "var(--color-board-light)" : "var(--color-board-dark)",
          }}
        >
          <SetPiece code={c} set={set} />
        </div>
      ))}
    </div>
  );
}

/**
 * A layer over a real `Board` (inside a `BoardSize`), one cell per square as drawn. `cells` puts
 * content on squares; `children` draws free SVG in square units (0..8), for motion traces.
 */
export function BoardOverlay({
  cells = {},
  orientation = "white",
  children,
}: {
  cells?: Record<string, ReactNode>;
  orientation?: "white" | "black";
  children?: ReactNode;
}) {
  return (
    <div
      aria-hidden="true"
      style={{ position: "absolute", inset: 0, pointerEvents: "none", zIndex: 5 }}
    >
      {Object.entries(cells).map(([square, node]) => {
        const { col, row } = cell(square, orientation);
        return (
          <div
            key={square}
            style={{
              position: "absolute",
              left: `${col * 12.5}%`,
              top: `${row * 12.5}%`,
              width: "12.5%",
              height: "12.5%",
            }}
          >
            {node}
          </div>
        );
      })}
      {children ? (
        <svg
          aria-hidden="true"
          viewBox="0 0 8 8"
          width="100%"
          height="100%"
          style={{ position: "absolute", inset: 0, overflow: "visible" }}
        >
          {children}
        </svg>
      ) : null}
    </div>
  );
}

/** A piece drawn on a square of the overlay, as a 30% ghost or solid, at the board's scale. */
export function OverlayPiece({
  code,
  ghost = false,
  set = "cburnett",
}: {
  code: string;
  ghost?: boolean;
  set?: PieceSetName;
}) {
  return (
    <div style={{ position: "absolute", inset: "8%", opacity: ghost ? 0.3 : 1 }}>
      <SetPiece code={code} set={set} />
    </div>
  );
}

/**
 * A storyboard motion trace (not a chess mark): a dashed pencil curve from one square centre to
 * another with an open head, used only in frame sequences to show where a piece travelled.
 */
export function MotionTrace({
  from,
  to,
  orientation = "white",
}: {
  from: string;
  to: string;
  orientation?: "white" | "black";
}) {
  const a = cell(from, orientation);
  const b = cell(to, orientation);
  const ax = a.col + 0.5;
  const ay = a.row + 0.5;
  const bx = b.col + 0.5;
  const by = b.row + 0.5;
  const mx = (ax + bx) / 2;
  const my = (ay + by) / 2 - 0.45;
  return (
    <g fill="none" strokeLinecap="round">
      <path
        d={`M${ax} ${ay} Q${mx} ${my} ${bx} ${by}`}
        stroke="var(--color-sheet)"
        strokeWidth={0.12}
      />
      <path
        d={`M${ax} ${ay} Q${mx} ${my} ${bx} ${by}`}
        stroke="var(--color-pencil)"
        strokeWidth={0.05}
        strokeDasharray="0.12 0.1"
      />
      <circle
        cx={bx}
        cy={by}
        r={0.09}
        fill="var(--color-pencil)"
        stroke="var(--color-sheet)"
        strokeWidth={0.03}
      />
    </g>
  );
}
