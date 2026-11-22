import { Chess } from "chess.js";

// The position explorer's model (feat/position-explorer): three lines through one cited position.
// The game line is the moves around the cited ply; the engine line shares the game's moves up to
// the cited ply and then follows the stored engine line; the tried line shares the shown line up
// to where the player started trying, then holds their own moves. The cursor counts the moves of
// the shown line already played, so 0 is the position before the first one.

export interface ExplorerMove {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  fenBefore: string;
  fenAfter: string;
}

/** The expanded data of one cited position: the game around it and the engine's stored line. */
export interface PositionLine {
  /** Game moves in order around the cited ply (it is among them). */
  game: ExplorerMove[];
  /** The stored engine line from the position before the cited move, or [] when none is stored. */
  engine: ExplorerMove[];
}

export type ExplorerMode = "game" | "engine" | "try";

export interface ExplorerState {
  ply: number;
  line: PositionLine;
  mode: ExplorerMode;
  cursor: number;
  /** In try mode, how many moves of the line the trying started from. */
  tryFrom: number;
  /** The moves the player tried after `tryFrom`. */
  tried: ExplorerMove[];
  /** The line the trying branched from. */
  tryBase: Exclude<ExplorerMode, "try">;
}

export type ExplorerAction =
  | { type: "step"; by: number }
  | { type: "start" }
  | { type: "end" }
  | { type: "goto"; ply: number }
  | { type: "mode"; mode: ExplorerMode }
  | { type: "play"; move: ExplorerMove }
  | { type: "reset" }
  | { type: "loaded"; line: PositionLine };

const UCI = /^[a-h][1-8][a-h][1-8][qrbn]?$/;

/** `uci` played on `fen` as an explorer move numbered `ply`, or null when it is not legal there. */
export function moveFrom(fen: string, uci: string, ply: number): ExplorerMove | null {
  if (!UCI.test(uci)) return null;
  try {
    const chess = new Chess(fen);
    const m = chess.move({ from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] });
    return {
      ply,
      color: m.color === "w" ? "white" : "black",
      san: m.san,
      uci: `${m.from}${m.to}${m.promotion ?? ""}`,
      fenBefore: fen,
      fenAfter: chess.fen(),
    };
  } catch {
    return null;
  }
}

/** The line known before anything is loaded: the move played, and the engine's better move. */
export function derivedLine(p: {
  ply: number;
  fenBefore: string;
  played: { uci: string; san?: string };
  best: { uci: string; san?: string } | null;
}): PositionLine {
  const played = moveFrom(p.fenBefore, p.played.uci, p.ply);
  const best =
    p.best && p.best.uci !== p.played.uci ? moveFrom(p.fenBefore, p.best.uci, p.ply) : null;
  return { game: played ? [played] : [], engine: best ? [best] : [] };
}

/** How many game moves come before the cited one. */
function rootOf(line: PositionLine, ply: number): number {
  const i = line.game.findIndex((m) => m.ply === ply);
  return i < 0 ? 0 : i;
}

function baseMoves(state: ExplorerState, mode: Exclude<ExplorerMode, "try">): ExplorerMove[] {
  if (mode === "game") return state.line.game;
  return [...state.line.game.slice(0, rootOf(state.line, state.ply)), ...state.line.engine];
}

/** The shown line's moves. */
export function lineMoves(state: ExplorerState): ExplorerMove[] {
  if (state.mode !== "try") return baseMoves(state, state.mode);
  return [...baseMoves(state, state.tryBase).slice(0, state.tryFrom), ...state.tried];
}

/** The FEN after `cursor` moves of `moves`; `start` when none are played. */
export function positionAt(moves: readonly ExplorerMove[], cursor: number, start: string): string {
  if (cursor <= 0) return moves[0]?.fenBefore ?? start;
  return moves[Math.min(cursor, moves.length) - 1]?.fenAfter ?? start;
}

export function initialExplorerState(line: PositionLine, ply: number): ExplorerState {
  return {
    ply,
    line,
    mode: "game",
    cursor: rootOf(line, ply),
    tryFrom: 0,
    tried: [],
    tryBase: "game",
  };
}

const clamp = (n: number, max: number) => Math.min(max, Math.max(0, n));

export function explorerReducer(state: ExplorerState, action: ExplorerAction): ExplorerState {
  const moves = lineMoves(state);
  switch (action.type) {
    case "step":
      return { ...state, cursor: clamp(state.cursor + action.by, moves.length) };
    case "start":
      return { ...state, cursor: 0 };
    case "end":
      return { ...state, cursor: moves.length };
    case "goto": {
      const i = moves.findIndex((m) => m.ply === action.ply);
      return i < 0 ? state : { ...state, cursor: i + 1 };
    }
    case "mode": {
      if (action.mode === state.mode) return state;
      if (action.mode === "try") {
        const tryBase = state.mode === "try" ? state.tryBase : state.mode;
        return { ...state, mode: "try", tryBase, tryFrom: state.cursor, tried: [] };
      }
      return { ...state, mode: action.mode, cursor: rootOf(state.line, state.ply), tried: [] };
    }
    case "play": {
      // A move on the board: in try mode it replaces what came after the cursor; from a line it
      // starts trying at the shown position.
      const next =
        state.mode === "try"
          ? state
          : {
              ...state,
              mode: "try" as const,
              tryBase: state.mode,
              tryFrom: state.cursor,
              tried: [],
            };
      const tryFrom = Math.min(next.tryFrom, state.cursor);
      const kept =
        state.cursor > next.tryFrom ? next.tried.slice(0, state.cursor - next.tryFrom) : [];
      return { ...next, tryFrom, tried: [...kept, action.move], cursor: state.cursor + 1 };
    }
    case "reset":
      if (state.mode !== "try") return state;
      return { ...state, tried: [], cursor: state.tryFrom };
    case "loaded": {
      // The cited position stays on the board: the cursor moves by the moves added before it.
      const shift = rootOf(action.line, state.ply) - rootOf(state.line, state.ply);
      const tryFrom = state.mode === "try" ? state.tryFrom + shift : state.tryFrom;
      return { ...state, line: action.line, cursor: Math.max(0, state.cursor + shift), tryFrom };
    }
  }
}

/** Whether the first tried move is the game's move or the engine's, at that position. */
export function tryMatch(state: ExplorerState): "game" | "engine" | null {
  if (state.mode !== "try") return null;
  const first = state.tried[0];
  if (!first) return null;
  const at = (moves: ExplorerMove[]) => moves[state.tryFrom];
  const game = at(baseMoves(state, "game"));
  const engine = at(baseMoves(state, "engine"));
  if (engine && engine.fenBefore === first.fenBefore && engine.uci === first.uci) return "engine";
  if (game && game.fenBefore === first.fenBefore && game.uci === first.uci) return "game";
  return null;
}
