import type { EngineScore, TimeClass } from "@ci/chess-core";

export interface NormalizedGame {
  platform: "lichess" | "chesscom";
  platformGameId: string;
  pgn: string;
  playedAt: Date;
  timeClass: TimeClass;
  baseSeconds: number | null;
  incrementSeconds: number | null;
  rated: boolean;
  white: { name: string; rating: number | null };
  black: { name: string; rating: number | null };
  result: "1-0" | "0-1" | "1/2-1/2" | "*";
  termination: string | null;
  eco: string | null;
  openingName: string | null;
  /** lichessEvals[i] is the white-POV eval after ply i+1. */
  lichessEvals: (EngineScore | null)[] | null;
}
