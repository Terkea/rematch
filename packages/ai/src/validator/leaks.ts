// Deliberately aggressive leak detector (ADR-AI-8). Runs on token-stripped text; anything it flags is a
// chess fact, number or date written as free text. False positives are acceptable (the model can repair
// by using a reference token); false negatives are not.
import { stripInvisibles } from "./normalize.js";

export type LeakKind =
  | "digit"
  | "percent"
  | "number_word"
  | "square"
  | "san"
  | "castling"
  | "figurine"
  | "date";

export interface Leak {
  kind: LeakKind;
  text: string;
}

// Cyrillic and Greek lookalikes folded to Latin so "е4" (Cyrillic е) cannot dodge the detectors.
const CONFUSABLES: Record<string, string> = {
  а: "a",
  е: "e",
  о: "o",
  р: "p",
  с: "c",
  х: "x",
  і: "i",
  ѕ: "s",
  һ: "h",
  в: "b",
  ο: "o",
  α: "a",
  ε: "e",
  ρ: "p",
  Α: "A",
  Β: "B",
  Ε: "E",
  Κ: "K",
  Ν: "N",
  Ο: "O",
  Ρ: "P",
  А: "A",
  В: "B",
  Е: "E",
  К: "K",
  Н: "H",
  О: "O",
  Р: "P",
  С: "C",
  Х: "X",
};

function canonical(s: string): string {
  const t = stripInvisibles(stripInvisibles(s).normalize("NFKC"));
  return t.replace(/[Α-ωЀ-ӿ]/g, (c) => CONFUSABLES[c] ?? c);
}

const DASH = "\\-\\u2010-\\u2015\\u2212\\u2796\\uFE58\\uFE63\\uFF0D";

const NUMBER_WORDS = [
  "zero",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
  "thirteen",
  "fourteen",
  "fifteen",
  "sixteen",
  "seventeen",
  "eighteen",
  "nineteen",
  "twenty",
  "thirty",
  "forty",
  "fourty",
  "fifty",
  "sixty",
  "seventy",
  "eighty",
  "ninety",
  "hundred",
  "thousand",
  "million",
  "billion",
  "dozen",
  "couple",
  "pair",
  "once",
  "twice",
  "thrice",
  "double",
  "doubled",
  "triple",
  "tripled",
  "quadruple",
  "half",
  "halves",
  "third",
  "quarter",
  "fourth",
  "fifth",
  "sixth",
  "seventh",
  "eighth",
  "ninth",
  "tenth",
  "eleventh",
  "twelfth",
  "thirteenth",
  "fourteenth",
  "fifteenth",
  "sixteenth",
  "seventeenth",
  "eighteenth",
  "nineteenth",
  "twentieth",
  "thirtieth",
  "fortieth",
  "fiftieth",
  "sixtieth",
  "seventieth",
  "eightieth",
  "ninetieth",
  "hundredth",
  "thousandth",
  "first",
  "second",
  "percent",
  "percentage",
]
  .sort((a, b) => b.length - a.length)
  .join("|");

const NUMBER_WORD_RE = new RegExp(
  `(?<![a-z])(?:(?:${NUMBER_WORDS})(?:s|es)?|(?:twen|thir|for|fif|six|seven|eigh|nine)ties|per\\s+cent)(?![a-z])`,
  "g",
);

const MONTH =
  "(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sept?(?:ember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";

interface Detector {
  kind: LeakKind;
  re: RegExp;
}

const DETECTORS: Detector[] = [
  { kind: "digit", re: /\p{N}+/gu },
  { kind: "percent", re: /[%‰٪]/g },
  { kind: "number_word", re: NUMBER_WORD_RE },
  { kind: "square", re: /[a-h][1-8]/g },
  { kind: "san", re: new RegExp(`[a-h][1-8][x${DASH}][a-h][1-8]`, "g") },
  { kind: "san", re: /[a-h][18]\s*[=(]?\s*[qrbn]\)?(?![a-z])/g },
  { kind: "san", re: /(?<![a-z])e\.?\s?p\.(?![a-z])/g },
  {
    kind: "castling",
    re: new RegExp(
      `(?<![a-z0-9])[o0]\\s*[${DASH}]+\\s*[o0](?:\\s*[${DASH}]+\\s*[o0])?(?![a-z0-9])`,
      "g",
    ),
  },
  { kind: "figurine", re: /[♔-♟\u{1FA00}-\u{1FA6F}]/gu },
  {
    kind: "date",
    re: new RegExp(
      `(?<![a-z])${MONTH}\\.?\\s+(?:\\d|the\\b)|\\d{1,2}(?:st|nd|rd|th)?\\s+(?:of\\s+)?${MONTH}(?![a-z])|(?<![a-z])(?:\\w+\\s+of\\s+)${MONTH}(?![a-z])|\\d{1,2}\\s*[/.]\\s*\\d{1,2}|\\d{4}\\s*-\\s*\\d{2}\\s*-\\s*\\d{2}`,
      "g",
    ),
  },
];

export function findLeaks(stripped: string): Leak[] {
  const c = canonical(stripped);
  const lc = c.toLowerCase();
  const out: Leak[] = [];
  const seen = new Set<string>();
  for (const d of DETECTORS) {
    const src = d.kind === "figurine" || d.kind === "digit" || d.kind === "percent" ? c : lc;
    for (const m of src.matchAll(new RegExp(d.re.source, d.re.flags))) {
      const k = `${d.kind}\u0000${m[0]}`;
      if (seen.has(k)) continue;
      seen.add(k);
      out.push({ kind: d.kind, text: m[0] });
    }
  }
  return out;
}
