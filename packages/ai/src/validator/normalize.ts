// Text normalisation applied before any extraction, so notation tricks cannot smuggle moves or numbers.

const INVISIBLES = /[­؜᠎​-‏‪-‮⁠-⁤﻿]/g;
const MINUS_BEFORE_DIGIT = /[‐-―−➖](?=\d)/g;
const FIGURINES: Record<string, string> = {
  "♔": "K",
  "♕": "Q",
  "♖": "R",
  "♗": "B",
  "♘": "N",
  "♚": "K",
  "♛": "Q",
  "♜": "R",
  "♝": "B",
  "♞": "N",
};

export function normalizeText(s: string): string {
  let t = s.replace(INVISIBLES, "").normalize("NFKC").replace(INVISIBLES, "");
  t = t.replace(MINUS_BEFORE_DIGIT, "-");
  t = t.replace(/[♔♕♖♗♘♚♛♜♝♞]/g, (c) => FIGURINES[c] ?? c);
  t = t.replace(/(?<![A-Za-z0-9])0-0(-0)?(?![A-Za-z0-9])/g, (_m, q: string | undefined) =>
    q ? "O-O-O" : "O-O",
  );
  return t;
}
