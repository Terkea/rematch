export type BoardSound =
  | "move"
  | "capture"
  | "check"
  | "castle"
  | "promote"
  | "gameEnd"
  | "correct"
  | "wrong"
  | "pen";

/** Which sound a move makes. Precedence: gameEnd (#) > check (+) > promote > castle > capture > move. */
export function soundForMove(move: { san: string; flags: string }): BoardSound {
  const { san, flags } = move;
  if (san.endsWith("#")) return "gameEnd";
  if (san.endsWith("+")) return "check";
  if (flags.includes("p")) return "promote";
  if (flags.includes("k") || flags.includes("q")) return "castle";
  if (flags.includes("c") || flags.includes("e")) return "capture";
  return "move";
}

/** The sound for a move known only by its SAN, such as a review's move list. */
export function soundForSan(san: string): BoardSound {
  const flags = san.startsWith("O-O-O")
    ? "q"
    : san.startsWith("O-O")
      ? "k"
      : `${san.includes("x") ? "c" : "n"}${san.includes("=") ? "p" : ""}`;
  return soundForMove({ san, flags });
}
