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

/** Every match of `re` (global) over the whole text, as hits. */
function scan(
  text: string,
  re: RegExp,
  pick = 0,
  offsetOf?: (m: RegExpExecArray) => number,
): BanHit[] {
  const hits: BanHit[] = [];
  const g = new RegExp(re.source, re.flags.includes("g") ? re.flags : `${re.flags}g`);
  for (let m = g.exec(text); m; m = g.exec(text)) {
    const at = offsetOf ? offsetOf(m) : m.index;
    hits.push({ line: lineOf(text, at), match: m[pick] ?? m[0] });
    if (m[0] === "") g.lastIndex++;
  }
  return hits;
}

const perLine = (text: string, fn: (line: string) => string[]): BanHit[] =>
  text.split("\n").flatMap((line, i) => fn(line).map((match) => ({ line: i + 1, match })));

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
    String.raw`](` +
    String.raw`(?:${BANNED_MODULES.join("|")})(?:/[^"'` +
    "`" +
    String.raw`]*)?)["'` +
    "`" +
    "]",
);

const QUOTED = /"[^"]*"|'[^']*'|`[^`]*`/g;
const FONT_IMPORT = /import\s*(?:type\s*)?\{([^}]*)\}\s*from\s*["']next\/font\/[a-z-]+["']/g;
const BANNED_FONTS = new Set(["Inter", "Geist", "Space_Grotesk", "Instrument_Serif"]);
const HEX = /(?<![\w&])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})\b/g;
const TOKEN_OR_BOARD_EXEMPT = /(^|\/)(board|annotations)\//;

const isCommentLine = (line: string) => /^\s*(\/\/|\/\*|\*)/.test(line);

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
    test: ({ text }) =>
      perLine(text, (line) =>
        (line.match(QUOTED) ?? [])
          .filter(
            (s) =>
              /(?<![\w-])uppercase(?![\w-])/.test(s) &&
              /(?<![\w-])(?:[\w:[\]-]*:)?tracking-/.test(s),
          )
          .map((s) => s.slice(1, -1)),
      ),
  },
  {
    id: "B5b",
    description: "No arrow glyph in tsx text or strings",
    test: ({ path, text }) =>
      norm(path).endsWith(".tsx")
        ? perLine(text, (line) => (!isCommentLine(line) && line.includes("→") ? ["→"] : []))
        : [],
  },
  {
    id: "B5c",
    description: "No near-black #0B0B0B / #111 / #111111; ink is a token",
    test: ({ text }) => scan(text, /#0b0b0b\b|#111111\b|#111\b/i),
  },
  {
    id: "B6",
    description: "No Inter, Geist, Space_Grotesk or Instrument_Serif from next/font",
    test: ({ text }) => {
      const hits: BanHit[] = [];
      const g = new RegExp(FONT_IMPORT.source, "g");
      for (let m = g.exec(text); m; m = g.exec(text)) {
        const body = m[1] ?? "";
        const bodyStart = m.index + m[0].indexOf("{") + 1;
        const name = /[A-Za-z_][A-Za-z0-9_]*/g;
        for (let n = name.exec(body); n; n = name.exec(body)) {
          // For `Foo as Bar` the imported name is the first identifier.
          const before = body.slice(0, n.index).trimEnd();
          if (/\bas$/.test(before)) continue;
          if (BANNED_FONTS.has(n[0]))
            hits.push({ line: lineOf(text, bodyStart + n.index), match: n[0] });
        }
      }
      return hits;
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
      return scan(text, HEX);
    },
  },
  {
    id: "TOK2",
    description: "No arbitrary Tailwind pixel lengths outside board/** and annotations/**",
    test: ({ path, text }) =>
      TOKEN_OR_BOARD_EXEMPT.test(norm(path))
        ? []
        : scan(text, /(?<!@(?:min|max))-\[-?\d+(?:\.\d+)?px\]/),
  },
];
