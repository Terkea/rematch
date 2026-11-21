// Text normalisation applied before any extraction, so notation tricks cannot smuggle moves or numbers.

/** Zero-width, bidi, joiner, variation-selector, filler and other invisible code points. */
const INVISIBLES = new RegExp(
  [
    "[\\u00AD\\u034F\\u061C\\u115F\\u1160\\u17B4\\u17B5\\u180B-\\u180E\\u200B-\\u200F\\u202A-\\u202E\\u2060-\\u2069\\u3164\\uFE00-\\uFE0F\\uFEFF\\uFFA0]",
    "[\\u{E0000}-\\u{E007F}\\u{E0100}-\\u{E01EF}]",
  ].join("|"),
  "gu",
);
const DASH = "\\u2010-\\u2015\\u2212\\u2796\\uFE58\\uFE63\\uFF0D";
const MINUS_BEFORE_DIGIT = new RegExp(`[${DASH}](?=\\d)`, "g");
const CASTLE_DASH = new RegExp(`(?<=[Oo0])[${DASH}](?=[Oo0])`, "g");
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

export function stripInvisibles(s: string): string {
  return s.replace(INVISIBLES, "");
}

export function normalizeText(s: string): string {
  // Space-grouped thousands ("1 000", nbsp / narrow nbsp / thin space) become "1,000" before NFKC erases the space kind.
  let t = stripInvisibles(s).replace(/(?<=\p{Nd})[    ](?=\p{Nd}{3}(?!\p{Nd}))/gu, ",");
  t = stripInvisibles(t.normalize("NFKC"));
  t = t.replace(CASTLE_DASH, "-");
  t = t.replace(MINUS_BEFORE_DIGIT, "-");
  t = t.replace(/[♔♕♖♗♘♚♛♜♝♞]/g, (c) => FIGURINES[c] ?? c);
  t = t.replace(/(?<![A-Za-z0-9])[O0]-[O0](-[O0])?(?![A-Za-z0-9])/g, (_m, q: string | undefined) =>
    q ? "O-O-O" : "O-O",
  );
  return t;
}
