export type BanFile = { path: string; text: string };
export type BanHit = { line: number; match: string };
export type BanRule = {
  id: string;
  description: string;
  test(file: BanFile): BanHit[];
};

const norm = (p: string) => p.replace(/[\\]/g, "/");

function lineOf(text: string, index: number): number {
  let n = 1;
  for (let i = 0; i < index; i++) if (text.charCodeAt(i) === 10) n++;
  return n;
}

/** Every match of `re` over the whole text, as hits. `accept` can veto a match. */
function scan(
  text: string,
  re: RegExp,
  pick = 0,
  accept?: (m: RegExpExecArray) => boolean,
): BanHit[] {
  const hits: BanHit[] = [];
  const g = new RegExp(re.source, re.flags.includes("g") ? re.flags : `${re.flags}g`);
  for (let m = g.exec(text); m; m = g.exec(text)) {
    if (m[0] === "") g.lastIndex++;
    if (accept && !accept(m)) continue;
    hits.push({ line: lineOf(text, m.index), match: m[pick] ?? m[0] });
  }
  return hits;
}

const perLine = (text: string, fn: (line: string) => string[]): BanHit[] =>
  text.split("\n").flatMap((line, i) => fn(line).map((match) => ({ line: i + 1, match })));

/** Blank out block comments (keeping newlines so line numbers hold). */
const maskBlockComments = (text: string) =>
  text.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));

const BANNED_MODULES = [
  "chessground",
  "@lichess-org/chessground",
  "chessops",
  "three",
  "@react-three/[^/\"'`]+",
  "aceternity",
  "magicui",
  "gsap",
  "lenis",
];
// from "x", import "x", import("x"), require("x"); subpaths are banned too.
const MODULE_RE = new RegExp(
  String.raw`(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*)["'` +
    "`" +
    `](` +
    `(?:${BANNED_MODULES.join("|")})(?:/[^"'` +
    "`" +
    `]*)?)["'` +
    "`" +
    "]",
);

const QUOTED = /"[^"\n]*"|'[^'\n]*'|`[^`]*`/g;
const BANNED_FONTS = new Set(["Inter", "Geist", "Space_Grotesk", "Instrument_Serif"]);
const FONT_IMPORT = /\bimport\s+(?:type\s+)?([^;"']*?)\s*from\s*["']next\/font\/[a-z-]+["']/g;
const HEX =
  /(?<=^|["'`(\s:,=[])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})\b/gm;
const TOKEN_OR_BOARD_EXEMPT = /(^|\/)(board|annotations)\//;

const isCommentLine = (line: string) => /^\s*(\/\/|\/\*|\*)/.test(line);

/** Text before index on its own line. */
function linePrefix(text: string, index: number): string {
  return text.slice(text.lastIndexOf("\n", index - 1) + 1, index);
}

/** `#abc` that is an id, anchor or SVG reference, not a colour. */
function isAnchorContext(text: string, index: number): boolean {
  const before = linePrefix(text, index);
  return (
    /\burl\(\s*["']?$/.test(before) ||
    /\b(?:href|xlinkHref|xlink:href|id|htmlFor|for|to|aria-[a-z]+|anchor|hash)\s*=\s*\{?\s*["'`]$/i.test(
      before,
    )
  );
}

/** Content between the bracket at `open` and its match, skipping string contents. */
function balanced(text: string, open: number): string {
  const pair: Record<string, string> = { "(": ")", "{": "}" };
  const o = text[open] ?? "(";
  const c = pair[o] ?? ")";
  let depth = 0;
  let quote = "";
  for (let i = open; i < text.length; i++) {
    const ch = text[i] as string;
    if (quote) {
      if (ch === "\\") i++;
      else if (ch === quote) quote = "";
    } else if (ch === '"' || ch === "'" || ch === "`") quote = ch;
    else if (ch === o) depth++;
    else if (ch === c && --depth === 0) return text.slice(open + 1, i);
  }
  return text.slice(open + 1);
}

const CLASS_CALL = /\b(?:cn|clsx|cva|twMerge|classNames)\s*\(|\bclassName\s*=\s*\{/g;

/** Class-string groups: every literal in a cn()/clsx()/className={} is joined; plain literals stand alone. */
function classGroups(text: string): { line: number; classes: string }[] {
  const groups: { line: number; classes: string }[] = [];
  const g = new RegExp(CLASS_CALL.source, "g");
  for (let m = g.exec(text); m; m = g.exec(text)) {
    const body = balanced(text, m.index + m[0].length - 1);
    const lits = (body.match(QUOTED) ?? []).map((s) => s.slice(1, -1));
    groups.push({ line: lineOf(text, m.index), classes: lits.join(" ") });
  }
  for (const m of text.matchAll(QUOTED)) {
    groups.push({ line: lineOf(text, m.index ?? 0), classes: m[0].slice(1, -1) });
  }
  return groups;
}

const UPPER = /(?<![\w-])uppercase(?![\w-])/;
const TRACK = /(?<![\w-])(?:[\w:[\]-]*:)?tracking-/;

const LENGTH = /(?<![a-z0-9.])-?(?:\d+\.?\d*|\.\d+)\s*(?:px|rem|em)(?![a-z0-9])/i;

export const banRules: BanRule[] = [
  {
    id: "B12",
    description:
      "No chessground, chessops, three, @react-three/*, aceternity, magicui, gsap or lenis (spec 11)",
    test: ({ text }) => scan(text, MODULE_RE, 1),
  },
  {
    id: "B13",
    description: "No lucide-react; icons come from @ci/ui/icons",
    test: ({ text }) =>
      scan(
        text,
        /(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*)["'`](lucide-react(?:\/[^"'`]*)?)["'`]/,
        1,
      ),
  },
  {
    id: "B5a",
    description: "No uppercase with tracking-* in one className (all-caps tracked labels)",
    test: ({ text }) => {
      const seen = new Set<string>();
      const hits: BanHit[] = [];
      for (const g of classGroups(text)) {
        if (UPPER.test(g.classes) && TRACK.test(g.classes)) {
          const key = `${g.line}:${g.classes}`;
          if (seen.has(key)) continue;
          seen.add(key);
          hits.push({ line: g.line, match: g.classes });
        }
      }
      return hits.sort((a, b) => a.line - b.line);
    },
  },
  {
    id: "B5b",
    description: "No arrow glyph (or &rarr;) in tsx text or strings",
    test: ({ path, text }) =>
      norm(path).endsWith(".tsx")
        ? perLine(maskBlockComments(text), (line) =>
            isCommentLine(line) ? [] : (line.match(/→|&rarr;|\\u2192/g) ?? []).map((m) => m),
          )
        : [],
  },
  {
    id: "B5c",
    description: "No near-black #0B0B0B / #111 / #111111; ink is a token",
    test: ({ text }) =>
      scan(
        text,
        /(?<=^|["'`(\s:,=[])#(?:0b0b0b|111111|111)\b/gim,
        0,
        (m) => !isAnchorContext(text, m.index),
      ),
  },
  {
    id: "B6",
    description:
      "No Inter, Geist, Space_Grotesk or Instrument_Serif from next/font (named, default or namespace)",
    test: ({ text }) => {
      const hits: BanHit[] = [];
      const g = new RegExp(FONT_IMPORT.source, "g");
      for (let m = g.exec(text); m; m = g.exec(text)) {
        const clause = m[1] ?? "";
        const clauseStart = m.index + m[0].indexOf(clause);
        const add = (name: string, at: number) =>
          hits.push({ line: lineOf(text, clauseStart + at), match: name });
        // Default import: the leading identifier before a comma or the end.
        const def = /^\s*([A-Za-z_$][\w$]*)\s*(?:,|$)/.exec(clause);
        if (def?.[1] && BANNED_FONTS.has(def[1])) add(def[1], clause.indexOf(def[1]));
        // Namespace import: `* as X`; X itself or X.Banned uses are flagged.
        const ns = /\*\s*as\s+([A-Za-z_$][\w$]*)/.exec(clause);
        if (ns?.[1]) {
          if (BANNED_FONTS.has(ns[1])) add(ns[1], clause.indexOf(ns[1]));
          const use = new RegExp(
            `\\b${ns[1].replace(/\$/g, "\\$")}\\.(${[...BANNED_FONTS].join("|")})\\b`,
            "g",
          );
          for (let u = use.exec(text); u; u = use.exec(text)) {
            hits.push({ line: lineOf(text, u.index), match: u[1] ?? u[0] });
          }
        }
        // Named imports: `Foo as Bar` imports Foo.
        const named = /\{([^}]*)\}/.exec(clause);
        if (named) {
          const body = named[1] ?? "";
          const bodyStart = clause.indexOf("{") + 1;
          const name = /[A-Za-z_][A-Za-z0-9_]*/g;
          for (let n = name.exec(body); n; n = name.exec(body)) {
            if (/\bas$/.test(body.slice(0, n.index).trimEnd())) continue;
            if (BANNED_FONTS.has(n[0])) add(n[0], bodyStart + n.index);
          }
        }
      }
      return hits.sort((a, b) => a.line - b.line);
    },
  },
  {
    id: "B7",
    description: "No gradient backgrounds, gradient text or backdrop blur",
    test: ({ text }) =>
      scan(text, /(?<![\w-])(?:bg-gradient-|bg-linear-|bg-radial-|bg-clip-text|backdrop-blur)/),
  },
  {
    id: "B15",
    description: "No lorem ipsum placeholder copy",
    test: ({ text }) => scan(text, /lorem/i),
  },
  {
    id: "B18",
    description: "No animate-spin spinners",
    test: ({ text }) => scan(text, /animate-spin/),
  },
  {
    id: "TOK1",
    description: "No raw hex outside tokens.css, board/** and annotations/**",
    test: ({ path, text }) => {
      const p = norm(path);
      if (p.endsWith("/tokens.css") || p === "tokens.css" || TOKEN_OR_BOARD_EXEMPT.test(p))
        return [];
      return scan(text, HEX, 0, (m) => !isAnchorContext(text, m.index));
    },
  },
  {
    id: "TOK2",
    description:
      "No arbitrary Tailwind lengths (px, rem, em; [prop:len]; calc) outside board/** and annotations/**",
    test: ({ path, text }) => {
      if (TOKEN_OR_BOARD_EXEMPT.test(norm(path))) return [];
      return scan(text, /\[([^\]\n]*)\]/, 0, (m) => {
        const prev = text[m.index - 1] ?? "";
        const content = m[1] ?? "";
        const isLen = prev === "-" && !/@(?:min|max)-$/.test(text.slice(0, m.index));
        const isProp = /^[a-z-]+:/i.test(content) && !/[\w\]]/.test(prev);
        return (isLen || isProp) && LENGTH.test(content);
      });
    },
  },
];

const ALLOW = /ban-lint-allow\s+([A-Za-z0-9]+)\s*:([^\n]*)/;

/**
 * Allow comments: `// ban-lint-allow <RULE>: <reason>` silences that rule on its
 * own line and the line after. A missing reason means the comment is ignored.
 */
export function allowedLines(text: string): Map<string, Set<number>> {
  const out = new Map<string, Set<number>>();
  text.split("\n").forEach((line, i) => {
    const m = ALLOW.exec(line);
    if (!m?.[1] || !m[2]?.replace(/\*\/\}?|-->|\}\s*$/g, "").trim()) return;
    const set = out.get(m[1]) ?? new Set<number>();
    set.add(i + 1);
    set.add(i + 2);
    out.set(m[1], set);
  });
  return out;
}

export function lintFile(file: BanFile): { rule: string; line: number; match: string }[] {
  const allow = allowedLines(file.text);
  return banRules.flatMap((rule) =>
    rule
      .test(file)
      .filter((h) => !allow.get(rule.id)?.has(h.line))
      .map((h) => ({ rule: rule.id, line: h.line, match: h.match })),
  );
}
