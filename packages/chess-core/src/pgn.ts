import { Chess } from "chess.js";

export type Color = "white" | "black";

export interface Ply {
  /** 1-based ply number. */
  ply: number;
  color: Color;
  san: string;
  uci: string;
  fenBefore: string;
  fenAfter: string;
  /** The mover's clock after the move, in seconds. */
  clockSeconds: number | null;
}

export interface ParsedGame {
  headers: Record<string, string>;
  startFen: string;
  plies: Ply[];
  terminal: "checkmate" | "stalemate" | "draw" | null;
}

export class UnsupportedVariantError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UnsupportedVariantError";
  }
}

const HEADER_RE = /^\s*\[(\w+)\s+"((?:[^"\\]|\\.)*)"\]\s*$/;
const CLOCK_RE = /\[%clk (\d+):(\d+):(\d+(?:\.\d+)?)\]/;
const STANDARD_VARIANTS = new Set(["standard", "chess"]);
const NON_MOVE_TOKEN_RE = /^(\d+\.+|\$\d+|1-0|0-1|1\/2-1\/2|\*)$/;

function splitPgn(pgn: string): { headers: Record<string, string>; movetext: string } {
  const headers: Record<string, string> = {};
  const body: string[] = [];
  let inHeaders = true;
  for (const line of pgn.split(/\r?\n/)) {
    const m = inHeaders ? HEADER_RE.exec(line) : null;
    if (m?.[1] !== undefined && m[2] !== undefined) {
      headers[m[1]] = m[2].replace(/\\(["\\])/g, "$1");
    } else if (inHeaders && line.trim() === "") {
      // blank lines between or after the header block
    } else {
      inHeaders = false;
      body.push(line);
    }
  }
  return { headers, movetext: body.join("\n") };
}

/** Clock comment for each move, by move index, read from the raw movetext. */
function extractClocks(movetext: string): (number | null)[] {
  const clocks: (number | null)[] = [];
  const withoutVariations = movetext.replace(/\([^()]*\)/g, " ");
  for (const m of withoutVariations.matchAll(/\{([^}]*)\}|;[^\n]*|(\S+)/g)) {
    const comment = m[1];
    const token = m[2];
    if (comment !== undefined) {
      const c = CLOCK_RE.exec(comment);
      if (c && clocks.length > 0) {
        clocks[clocks.length - 1] = Number(c[1]) * 3600 + Number(c[2]) * 60 + Number(c[3]);
      }
    } else if (token !== undefined && !NON_MOVE_TOKEN_RE.test(token)) {
      // "12.e4" style tokens glue the move number to the move.
      clocks.push(null);
    }
  }
  return clocks;
}

export function parsePgn(pgn: string): ParsedGame {
  const { headers, movetext } = splitPgn(pgn);

  const variant = headers.Variant;
  if (variant !== undefined && !STANDARD_VARIANTS.has(variant.trim().toLowerCase())) {
    throw new UnsupportedVariantError(`Unsupported variant: ${variant}`);
  }
  if (headers.SetUp !== undefined || headers.FEN !== undefined) {
    throw new UnsupportedVariantError("Games from a custom starting position are not supported");
  }

  const loaded = new Chess();
  loaded.loadPgn(movetext);
  const moves = loaded.history({ verbose: true });
  const clocks = extractClocks(movetext);

  const replay = new Chess();
  const startFen = replay.fen();
  const plies: Ply[] = moves.map((move, i) => ({
    ply: i + 1,
    color: move.color === "w" ? "white" : "black",
    san: move.san,
    uci: `${move.from}${move.to}${move.promotion ?? ""}`,
    fenBefore: move.before,
    fenAfter: move.after,
    clockSeconds: clocks.length === moves.length ? (clocks[i] ?? null) : null,
  }));

  let terminal: ParsedGame["terminal"] = null;
  if (loaded.isCheckmate()) terminal = "checkmate";
  else if (loaded.isStalemate()) terminal = "stalemate";
  else if (loaded.isDraw()) terminal = "draw";

  return { headers, startFen, plies, terminal };
}
