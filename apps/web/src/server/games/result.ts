import type { PlayerResult } from "./types";

export function playerResult(
  result: "1-0" | "0-1" | "1/2-1/2" | "*",
  colour: "white" | "black",
): PlayerResult {
  if (result === "*") return "unfinished";
  if (result === "1/2-1/2") return "draw";
  const whiteWon = result === "1-0";
  return whiteWon === (colour === "white") ? "win" : "loss";
}
