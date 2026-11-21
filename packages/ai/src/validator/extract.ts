import { normalizeText } from "./normalize.js";

export interface ExtractedNumber {
  value: number;
  decimals: number;
  percent: boolean;
  hedged: boolean;
  kind: "plain" | "ratio";
  /** True for forms such as "1,5" that could be a decimal comma or a list; the validator must treat it as unverifiable. */
  ambiguous: boolean;
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
const DASHES = "-\\u2010-\\u2015\\u2212";

// Long algebraic: e2-e4, Ng1-f3, e2xd5, Qd1–h5, e7-e8=Q
const longRe = () =>
  new RegExp(`${NB}[KQRBN]?(${SQ})[${DASHES}x](${SQ})(?:=?\\(?([QRBNqrbn])\\)?)?${NA}`, "g");
const uciRe = () => new RegExp(`${NB}(${SQ})(${SQ})([qrbn])?${NA}`, "g");

const SAN_SRC = [
  "(O-O-O|O-O)",
  `([KQRBN](?:${SQ}|[a-h]|[1-8])?x?${SQ})`,
  `([a-h]x${SQ}(?:=?\\(?[QRBN]\\)?)?)`,
  `([a-h][18]=?\\(?[QRBN]\\)?)`,
  `(${SQ})`,
].join("|");
const sanRe = () => new RegExp(`${NB}(?:${SAN_SRC})${NA}`, "g");

interface Hit {
  at: number;
  end: number;
  text: string;
}

const MOVE_NO_BEFORE = /(?:\d+\s*\.(?:\s*\.{1,3})?|\.{3})\s*$/;
const PLAY_BEFORE = /\b(?:play|played|playing|push|pushed|advance|advanced)\s+(?:to\s+)?$/i;
const GAP_AFTER_MOVE = /^[\s().,;:\-+#!?…]*$/;
const GAP_BEFORE_MOVE = /^\s*$/;

function normaliseSanText(s: string): string {
  const stripped = s.replace(/[()]/g, "");
  const m = /^([a-h](?:x[a-h])?[18])=?([QRBN])$/.exec(stripped);
  return m ? `${m[1]}=${m[2]}` : stripped;
}

/** Finds SAN and UCI-like moves and returns text with each hit masked by a placeholder. */
function scanMoves(t: string): { san: Hit[]; uci: Hit[]; masked: string } {
  const uci: Hit[] = [];
  let work = t.replace(
    longRe(),
    (m, from: string, to: string, p: string | undefined, off: number) => {
      uci.push({ at: off, end: off + m.length, text: `${from}${to}${p ? p.toLowerCase() : ""}` });
      return PH.repeat(m.length);
    },
  );
  work = work.replace(
    uciRe(),
    (m, from: string, to: string, p: string | undefined, off: number) => {
      uci.push({ at: off, end: off + m.length, text: `${from}${to}${p ?? ""}` });
      return PH.repeat(m.length);
    },
  );

  const cands = [...work.matchAll(sanRe())].map((m) => {
    const at = m.index ?? 0;
    return { at, end: at + m[0].length, text: m[0], bare: m[5] !== undefined };
  });
  const san: Hit[] = [];
  let lastEnd = -1;
  for (let i = 0; i < cands.length; i++) {
    const c = cands[i];
    if (!c) continue;
    let accept = !c.bare;
    if (c.bare) {
      const before = work.slice(Math.max(0, c.at - 16), c.at);
      if (MOVE_NO_BEFORE.test(before) || PLAY_BEFORE.test(before)) accept = true;
      else if (lastEnd >= 0 && GAP_AFTER_MOVE.test(work.slice(lastEnd, c.at))) accept = true;
      else {
        const next = cands[i + 1];
        if (next && !next.bare && GAP_BEFORE_MOVE.test(work.slice(c.end, next.at))) accept = true;
      }
    }
    if (accept) {
      san.push({ at: c.at, end: c.end, text: normaliseSanText(c.text) });
      lastEnd = c.end;
    }
  }
  const chars = work.split("");
  for (const h of san) for (let k = h.at; k < h.end; k++) chars[k] = PH;
  return { san, uci, masked: chars.join("") };
}

export function extractMoves(text: string): { san: string[]; uci: string[] } {
  const { san, uci } = scanMoves(normalizeText(text));
  const byPos = (a: Hit, b: Hit) => a.at - b.at;
  return {
    san: san.sort(byPos).map((h) => h.text),
    uci: uci.sort(byPos).map((h) => h.text),
  };
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

const ONES: Record<string, number> = {
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
};
const TENS: Record<string, number> = {
  twenty: 20,
  thirty: 30,
  forty: 40,
  fourty: 40,
  fifty: 50,
  sixty: 60,
  seventy: 70,
  eighty: 80,
  ninety: 90,
};
const WORD_ALT = [...Object.keys(ONES), ...Object.keys(TENS), "hundred", "thousand"]
  .sort((a, b) => b.length - a.length)
  .join("|");
const UNITS = new Set(
  [
    "game",
    "move",
    "piece",
    "blunder",
    "mistake",
    "time",
    "position",
    "week",
    "day",
    "point",
    "opening",
    "month",
    "session",
    "pawn",
    "error",
    "hour",
    "minute",
    "second",
    "year",
    "percent",
  ].flatMap((w) => [w, `${w}s`]),
);
UNITS.add("percent");
const FRACTION_DEN: Record<string, number> = {
  third: 3,
  quarter: 4,
  fourth: 4,
  fifth: 5,
  sixth: 6,
  seventh: 7,
  eighth: 8,
  ninth: 9,
  tenth: 10,
};
const HEDGE_RE =
  /(?:about|around|roughly|nearly|almost|over|under|approximately|approx\.?|more than|less than|at least|at most|~|∼)\s*$/i;

function hedgedBefore(t: string, idx: number): boolean {
  return HEDGE_RE.test(t.slice(Math.max(0, idx - 20), idx));
}

/** Evaluates a run of number words into one or more numbers (splitting at invalid sequences). */
function evalWordRun(words: string[], article: boolean): number[] {
  const out: number[] = [];
  let total = 0;
  let cur = 0;
  let last: "ones" | "tens" | "hundred" | "thousand" | null = null;
  let any = false;
  const flush = () => {
    if (any) out.push(total + cur);
    total = 0;
    cur = 0;
    last = null;
    any = false;
  };
  words.forEach((w, i) => {
    const art = article && i === 0;
    if (w in ONES) {
      const v = ONES[w] as number;
      if (last === "tens" && v >= 1 && v <= 9) cur += v;
      else if (cur % 100 === 0 && last !== "ones" && last !== "tens") cur += v;
      else {
        flush();
        cur = v;
      }
      last = last === "tens" && v >= 1 && v <= 9 ? "ones" : "ones";
      any = true;
    } else if (w in TENS) {
      if (cur % 100 !== 0 || last === "ones" || last === "tens") flush();
      cur += TENS[w] as number;
      last = "tens";
      any = true;
    } else if (w === "hundred") {
      if (any && cur > 0 && cur < 100) cur *= 100;
      else if (art || !any) {
        flush();
        cur = 100;
      } else cur += 0;
      last = "hundred";
      any = true;
    } else if (w === "thousand") {
      total += (cur || 1) * 1000;
      cur = 0;
      last = "thousand";
      any = true;
    }
  });
  flush();
  return out;
}

const QUANT_HALF_NEXT =
  /^\s+(?:of|as|a|an|the|your|my|our|their|these|those|all|again|more|less|fewer|over)\b/i;
const NONQ_HALF_PREV =
  /\b(?:first|second|other|latter|former|back|top|bottom|upper|lower|opening|closing|final|next|last)\s+$/i;

export function extractNumbers(text: string): ExtractedNumber[] {
  let t = normalizeText(text);
  t = t.replace(/\[\[g:[^\]]*\]\]/g, PH);
  t = scanDates(t).rest;
  t = scanMoves(t).masked;
  // move-number prefixes directly before a removed move
  t = t.replace(new RegExp(`${NB}\\d+\\s*\\.(?:\\s*\\.{1,3})?\\s*(?=${PH})`, "g"), PH);
  t = t.replace(/\bper\s+cent\b/gi, "percent");
  // weakness ids, band ids, game results, ordinals
  t = t.replace(/(?<![A-Za-z0-9])W\d{1,2}[a-d]?(?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])B[0-5](?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])(?:1-0|0-1|1(?:\/|⁄)2-1(?:\/|⁄)2)(?![A-Za-z0-9])/g, PH);
  t = t.replace(/(?<![A-Za-z0-9])\d+(?:st|nd|rd|th)(?![A-Za-z])/gi, PH);

  const found: { at: number; n: ExtractedNumber }[] = [];
  const mk = (
    at: number,
    value: number,
    o: Partial<ExtractedNumber> & { decimals?: number },
  ): ExtractedNumber => ({
    value,
    decimals: o.decimals ?? 0,
    percent: o.percent ?? false,
    hedged: hedgedBefore(t, at),
    kind: o.kind ?? "plain",
    ambiguous: o.ambiguous ?? false,
  });

  // digits
  const numRe =
    /(?<![A-Za-z0-9])([-+]?)(?:(\d{1,3}(?:,\d{3})+(?!\d)|\d+,\d+|\d+)(?:\.(\d+))?|\.(\d+))/g;
  for (const m of t.matchAll(numRe)) {
    const at = m.index ?? 0;
    let sign = m[1] ?? "";
    if (sign && t[at - 1] === "%") sign = ""; // "50%-60%" is a range
    const intRaw = m[2] ?? "";
    const frac = m[3] ?? m[4] ?? "";
    const ambiguous = /^\d+,\d+$/.test(intRaw) && !/^\d{1,3}(?:,\d{3})+$/.test(intRaw);
    let value: number;
    let decimals = frac.length;
    if (ambiguous) {
      const [a, b] = intRaw.split(",");
      value = Number(`${a}.${b}${frac}`);
      decimals = (b?.length ?? 0) + frac.length;
    } else {
      value = Number(`${intRaw.replace(/,/g, "") || "0"}${frac ? `.${frac}` : ""}`);
    }
    if (sign === "-") value = -value;
    const after = t.slice(at + m[0].length);
    const ratio = /^(?:x|×)(?![A-Za-z])|^\s*times\b/i.test(after);
    found.push({
      at,
      n: mk(at, value, {
        decimals,
        percent: /^\s*(?:%|percent\b)/i.test(after),
        kind: ratio ? "ratio" : "plain",
        ambiguous,
      }),
    });
  }

  // fractions ("a third", "two thirds", "three-quarters") and dozens
  const maskRanges: [number, number][] = [];
  const fracRe = new RegExp(
    `(?<![A-Za-z])(a|an|${Object.keys(ONES).join("|")})[\\s-]+(${Object.keys(FRACTION_DEN).join("|")})s?(?![A-Za-z])`,
    "gi",
  );
  for (const m of t.matchAll(fracRe)) {
    const at = m.index ?? 0;
    const q = (m[1] ?? "").toLowerCase();
    const num = q === "a" || q === "an" ? 1 : (ONES[q] as number);
    const den = FRACTION_DEN[(m[2] ?? "").toLowerCase()] as number;
    found.push({ at, n: mk(at, num / den, { kind: "ratio", decimals: 2 }) });
    maskRanges.push([at, at + m[0].length]);
  }
  const dozenRe = new RegExp(
    `(?<![A-Za-z])(a|an|${Object.keys(ONES).join("|")})\\s+dozen(?![A-Za-z])`,
    "gi",
  );
  for (const m of t.matchAll(dozenRe)) {
    const at = m.index ?? 0;
    const q = (m[1] ?? "").toLowerCase();
    const num = q === "a" || q === "an" ? 1 : (ONES[q] as number);
    found.push({ at, n: mk(at, num * 12, {}) });
    maskRanges.push([at, at + m[0].length]);
  }
  if (maskRanges.length) {
    const chars = t.split("");
    for (const [a, b] of maskRanges) for (let k = a; k < b; k++) chars[k] = PH;
    t = chars.join("");
  }

  // number words (with a counted unit within two tokens, not followed by "of")
  const runRe = new RegExp(
    `(?<![A-Za-z])((?:a\\s+)?)(${WORD_ALT})(?:(?:\\s*-\\s*|\\s+)(?:and\\s+)?(?:${WORD_ALT}))*(?![A-Za-z])`,
    "gi",
  );
  for (const m of t.matchAll(runRe)) {
    const at = m.index ?? 0;
    const end = at + m[0].length;
    const words = (m[0].toLowerCase().match(new RegExp(WORD_ALT, "gi")) ?? []).map((w) =>
      w.toLowerCase(),
    );
    const article = /^a\s/i.test(m[0]);
    const toks = t
      .slice(end)
      .match(/^(?:\s+[A-Za-z]+){1,2}/)?.[0]
      ?.trim()
      .split(/\s+/)
      .map((x) => x.toLowerCase());
    if (!toks || toks[0] === "of") continue;
    if (!toks.slice(0, 2).some((x) => UNITS.has(x))) continue;
    const vals = evalWordRun(words, article);
    const value = vals[vals.length - 1];
    if (value === undefined) continue;
    const isPct = toks.slice(0, 2).includes("percent");
    const ratio = toks[0] === "times" && value >= 3 && value <= 10;
    found.push({
      at,
      n: mk(at, value, { percent: isPct, kind: ratio ? "ratio" : "plain" }),
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
      if (value === 0.5) {
        const next = t.slice(at + m[0].length);
        const quantNext =
          QUANT_HALF_NEXT.test(next) ||
          /^(?:\s+[A-Za-z]+){0,1}\s+(?:games?|moves?|percent|points?)\b/i.test(next);
        if (!quantNext || NONQ_HALF_PREV.test(t.slice(Math.max(0, at - 12), at))) continue;
      }
      found.push({
        at,
        n: mk(at, value, { kind: "ratio", decimals: value % 1 ? 1 : 0 }),
      });
    }
  }

  found.sort((a, b) => a.at - b.at);
  return found.map((f) => f.n);
}
