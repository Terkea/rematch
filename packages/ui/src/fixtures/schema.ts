import { z } from "zod";

const fen = z.string().regex(/^\S+ [wb] \S+ \S+ \d+ \d+$/, "Expected a full FEN");
const uci = z.string().regex(/^[a-h][1-8][a-h][1-8][qrbn]?$/);

export const fixturePlySchema = z.object({
  ply: z.number().int().positive(),
  color: z.enum(["white", "black"]),
  san: z.string().min(1),
  uci,
  fenAfter: fen,
  winWhite: z.number().min(0).max(100),
  cpWhite: z.number().int().nullable(),
  mateWhite: z.number().int().nullable(),
  judgement: z.enum(["inaccuracy", "mistake", "blunder"]).nullable(),
  bestUci: uci.nullable(),
  clockSeconds: z.number().nonnegative().nullable(),
  phase: z.enum(["opening", "middlegame", "endgame"]),
});

const player = z.object({ name: z.string().min(1), rating: z.number().int().nullable() });

export const gameFixtureSchema = z.object({
  id: z.string().min(1),
  source: z.url(),
  white: player,
  black: player,
  playerColor: z.enum(["white", "black"]),
  result: z.enum(["1-0", "0-1", "1/2-1/2"]),
  timeClass: z.string().min(1),
  eco: z.string().nullable(),
  openingName: z.string().nullable(),
  startFen: fen,
  plies: z.array(fixturePlySchema).min(1),
});
