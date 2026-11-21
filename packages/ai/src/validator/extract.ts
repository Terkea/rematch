import { normalizeText } from "./normalize.js";

export interface ExtractedNumber {
  value: number;
  decimals: number;
  percent: boolean;
  hedged: boolean;
  kind: "plain" | "ratio";
}
export interface ExtractedDate {
  year: number | null;
  month: number;
  day: number;
}

const PH = "\u0001"; // placeholder for removed tokens (never alphanumeric)
const NB = "(?<![A-Za-z0-9])";
const NA = "(?![A-Za-z0-9])";
const SQ = "[a-h][1-8]";
const PUSH_CTX =
  "(?<=(?:\\d+\\.(?:\\s*\\.{1,3})?\\s*|\\b(?:play|played|playing|push|pushed|advance|advanced)\\s+(?:to\\s+)?))";

const SAN_SRC = [
  "(O-O-O|O-O)",
  `([KQRBN](?:${SQ}|[a-h]|[1-8])?x?${SQ})`,
  `([a-h]x${SQ}(?:=[QRBN])?)`,
  `([a-h][18]=[QRBN])`,
  `(${PUSH_CTX}${SQ})`,
].join("|");
const sanRe = () => new RegExp(`${NB}(?:${SAN_SRC})${NA}`, "g");
const uciRe = () => new RegExp(`${NB}${SQ}${SQ}[qrbn]?${NA}`, "g");

export function extractMoves(text: string): { san: string[]; uci: string[] } {
  const t = normalizeText(text);
  const uci = [...t.matchAll(uciRe())].map((m) => m[0]);
  const noUci = t.replace(uciRe(), PH);
  const san = [...noUci.matchAll(sanRe())].map((m) => m[0]);
  return { san, uci };
}

// ---- dates ----

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const MON_SRC =
  "(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?|Sept?(?:ember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)";
const YEAR_SRC = "((?:19|20)\\d{2})";
const ISO_RE = () => /(?<![\d-])(\d{4})-(\d{2})-(\d{2})(?![\d])/g;
const DMY_RE = () =>
  new RegExp(
    `${NB}(\\d{1,2})(?:st|nd|rd|th)?\\s+(${MON_SRC})\\b\\.?(?:,?\\s+${YEAR_SRC}${NA})?`,
    "gi",
  );
const MDY_RE = () =>
  new RegExp(
    `${NB}(${MON_SRC})\\b\\.?\\s+(\\d{1,2})(?:st|nd|rd|th)?(?![\\d:])(?:,?\\s+${YEAR_SRC}${NA})?`,
    "gi",
  );

function monthIndex(name: string): number | null {
  if (name.toLowerCase() === "may" && name !== "May") return null; // "you may 5 times"
  const i = MONTHS.indexOf(name.slice(0, 3).toLowerCase());
  return i < 0 ? null : i + 1;
}
const validDay = (m: number, d: number) => m >= 1 && m <= 12 && d >= 1 && d <= 31;

/** Finds dates; returns them and the text with each date replaced by a placeholder. */
function scanDates(t: string): { dates: ExtractedDate[]; rest: string } {
  const found: { at: number; d: ExtractedDate }[] = [];
  let rest = t;
  rest = rest.replace(ISO_RE(), (m, y: string, mo: string, d: string, off: number) => {
    const month = Number(mo);
    const day = Number(d);
    if (!validDay(month, day)) return m;
    found.push({ at: off, d: { year: Number(y), month, day } });
    return PH.repeat(m.length);
  });
  rest = rest.replace(DMY_RE(), (m, d: string, mon: string, y: string | undefined, off: number) => {
    const month = monthIndex(mon);
    const day = Number(d);
    if (month === null || !validDay(month, day)) return m;
    found.push({ at: off, d: { year: y ? Number(y) : null, month, day } });
    return PH.repeat(m.length);
  });
  rest = rest.replace(MDY_RE(), (m, mon: string, d: string, y: string | undefined, off: number) => {
    const month = monthIndex(mon);
    const day = Number(d);
    if (month === null || !validDay(month, day)) return m;
    found.push({ at: off, d: { year: y ? Number(y) : null, month, day } });
    return PH.repeat(m.length);
  });
  found.sort((a, b) => a.at - b.at);
  return { dates: found.map((f) => f.d), rest };
}

export function extractDates(text: string): ExtractedDate[] {
  return scanDates(normalizeText(text)).dates;
}

// ---- numbers ----

const WORDS: Record<string, number> = {
  zero: 0,
  one: 1,
  two: 2,
  three: 3,
  four: 4,
  five: 5,
  six: 6,
  seven: 7,
  eight: 8,
  nine: 9,
  ten: 10,
  eleven: 11,
  twelve: 12,
  thirteen: 13,
  fourteen: 14,
  fifteen: 15,
  sixteen: 16,
  seventeen: 17,
  eighteen: 18,
  nineteen: 19,
  twenty: 20,
};
const UNITS = new Set([
  "games",
  "moves",
  "pieces",
  "blunders",
  "mistakes",
  "times",
  "positions",
  "weeks",
  "days",
  "points",
  "percent",
]);
const HEDGE_RE =
  /(?:about|around|roughly|nearly|almost|over|under|approximately|approx\.?|more than|less than|at least|at most|~|∼)\s*$/i;

function hedgedBefore(t: string, idx: number): boolean {
  return HEDGE_RE.test(t.slice(Math.max(0, idx - 20), idx));
}

export function extractNumbers(text: string): ExtractedNumber[] {
  let t = normalizeText(text);
  t = t.replace(/\[\[g:[^\]]*\]\]/g, PH);
  t = scanDates(t).rest;
  t = t.replace(uciRe(), PH);
  t = t.replace(sanRe(), PH);
  // move-number prefixes directly before a removed move
  t = t.replace(new RegExp(`${NB}\\d+\\.(?:\\s*\\.{1,3})?\\s*(?=${PH})`, "g"), PH);
  // weakness ids, band ids, game results, ordinals
  t = t.replace(/(?<![A-Za-z0-9])W\d{1,2}[a-d]?(?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])B[0-5](?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])(?:1-0|0-1|1(?:\/|⁄)2-1(?:\/|⁄)2)(?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])\d+(?:st|nd|rd|th)(?![A-Za-z])/gi, PH);

  const found: { at: number; n: ExtractedNumber }[] = [];

  const numRe = /(?<![A-Za-z0-9])([-+]?)(\d{1,3}(?:,\d{3})+(?!\d)|\d+)(?:\.(\d+))?/g;
  for (const m of t.matchAll(numRe)) {
    const at = m.index ?? 0;
    const sign = m[1] ?? "";
    const intPart = (m[2] ?? "").replace(/,/g, "");
    const frac = m[3] ?? "";
    // a '.' directly before an unsigned number glued to a letter/digit run (e.g. "e5.3") is not a number
    const value = Number(`${intPart}${frac ? `.${frac}` : ""}`) * (sign === "-" ? -1 : 1);
    const after = t.slice(at + m[0].length);
    found.push({
      at,
      n: {
        value,
        decimals: frac.length,
        percent: /^\s*(?:%|percent\b|per cent\b)/i.test(after),
        hedged: hedgedBefore(t, at),
        kind: "plain",
      },
    });
  }

  const wordRe =
    /(?<![A-Za-z0-9])(zero|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty)(?![A-Za-z0-9])/gi;
  for (const m of t.matchAll(wordRe)) {
    const at = m.index ?? 0;
    const w = (m[1] ?? "").toLowerCase();
    const toks = t
      .slice(at + m[0].length)
      .match(/^(?:\s+[A-Za-z]+){1,2}/)?.[0]
      ?.trim()
      .split(/\s+/)
      .map((x) => x.toLowerCase());
    if (!toks || toks[0] === "of") continue;
    const unit = toks.slice(0, 2).find((x) => UNITS.has(x));
    if (!unit) continue;
    const value = WORDS[w] as number;
    const ratio = unit === "times" && toks[0] === "times" && value >= 3 && value <= 10;
    found.push({
      at,
      n: {
        value,
        decimals: 0,
        percent: unit === "percent",
        hedged: hedgedBefore(t, at),
        kind: ratio ? "ratio" : "plain",
      },
    });
  }

  const ratioWords: [RegExp, number][] = [
    [/(?<![A-Za-z0-9])twice(?![A-Za-z0-9])/gi, 2],
    [/(?<![A-Za-z0-9])double(?![A-Za-z0-9-])(?!\s+check)/gi, 2],
    [/(?<![A-Za-z0-9])half(?![A-Za-z0-9-])/gi, 0.5],
  ];
  for (const [re, value] of ratioWords) {
    for (const m of t.matchAll(re)) {
      const at = m.index ?? 0;
      found.push({
        at,
        n: {
          value,
          decimals: value % 1 ? 1 : 0,
          percent: false,
          hedged: hedgedBefore(t, at),
          kind: "ratio",
        },
      });
    }
  }

  found.sort((a, b) => a.at - b.at);
  return found.map((f) => f.n);
}
