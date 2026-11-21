// Build-time recolouring of a piece set (DESIGN.md, "Piece sets"): every dark paint becomes
// --color-piece-dark and every light paint --color-piece-light, so a theme switch recolours the
// pieces through CSS variables and nothing recolours at runtime with `filter`. Pure, so the board
// test can rebuild the set from the committed SVGs and fail on drift.

import type { PieceCode, PieceNode, PieceSet } from "../src/board/piece-types.js";

export type { PieceCode, PieceNode, PieceSet };

const roles = [
  ["K", "k"],
  ["Q", "q"],
  ["R", "r"],
  ["B", "b"],
  ["N", "n"],
  ["P", "p"],
] as const;

/** The twelve Commons files of the Cburnett set and the react-chessboard code each one fills. */
export const PIECE_FILES: readonly { code: PieceCode; file: string }[] = roles.flatMap(
  ([upper, lower]) => [
    { code: `w${upper}` as PieceCode, file: `Chess_${lower}lt45.svg` },
    { code: `b${upper}` as PieceCode, file: `Chess_${lower}dt45.svg` },
  ],
);

const DARK = "var(--color-piece-dark)";
const LIGHT = "var(--color-piece-light)";
const PAINT = new Set(["fill", "stroke"]);
const GEOMETRY = new Set(["d", "cx", "cy", "r", "transform"]);
// Ignored: XML and document attributes, and ids (they would collide across 32 pieces).
const IGNORED = new Set(["id", "xmlns", "version", "width", "height"]);
const PRESENTATION = new Set([
  "fill",
  "fill-opacity",
  "fill-rule",
  "opacity",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-miterlimit",
  "stroke-dasharray",
  "stroke-opacity",
]);

function paint(value: string): string {
  const v = value.trim().toLowerCase();
  if (v === "none") return "none";
  if (v === "#000" || v === "#000000") return DARK;
  if (v === "#fff" || v === "#ffffff") return LIGHT;
  throw new Error(`Unexpected paint ${value}: a set may use only two colours (DESIGN.md)`);
}

const camel = (name: string) => name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());

function presentation(name: string, value: string, into: Record<string, string>): void {
  if (!PRESENTATION.has(name)) throw new Error(`Unexpected presentation property ${name}`);
  into[camel(name)] = PAINT.has(name) ? paint(value) : value.trim();
}

function parseAttrs(source: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  for (const m of source.matchAll(/([\w:-]+)\s*=\s*"([^"]*)"/g)) {
    attrs[m[1] as string] = m[2] as string;
  }
  return attrs;
}

function toNode(tag: string, raw: Record<string, string>): PieceNode {
  if (tag !== "g" && tag !== "path" && tag !== "circle") throw new Error(`Unexpected <${tag}>`);
  const attrs: Record<string, string> = {};
  const style: Record<string, string> = {};
  for (const [name, value] of Object.entries(raw)) {
    if (IGNORED.has(name)) continue;
    if (GEOMETRY.has(name)) attrs[name] = value.replace(/\s+/g, " ").trim();
    else if (name === "style") {
      for (const decl of value.split(";")) {
        const [prop, val] = decl.split(":");
        if (prop?.trim() && val !== undefined) presentation(prop.trim(), val, style);
      }
    } else presentation(name, value, style);
  }
  return { tag, attrs, style, children: [] };
}

/** Parses one Cburnett SVG (a 45 × 45 drawing of g, path and circle) into recoloured nodes. */
export function parsePiece(svg: string): PieceNode[] {
  const body = svg.replace(/<\?xml[^>]*\?>/, "").replace(/<!DOCTYPE[^>]*>/, "");
  const root: PieceNode = { tag: "g", attrs: {}, style: {}, children: [] };
  const stack: PieceNode[] = [root];
  let sawSvg = false;
  for (const m of body.matchAll(/<(\/?)([a-zA-Z]+)([^>]*?)(\/?)>/g)) {
    const [, closing, tag, rest, selfClosing] = m as unknown as [
      string,
      string,
      string,
      string,
      string,
    ];
    if (tag === "svg") {
      if (!closing) {
        const { width, height } = parseAttrs(rest);
        if (width !== "45" || height !== "45") throw new Error("Expected a 45 × 45 drawing");
        sawSvg = true;
      }
      continue;
    }
    if (closing) {
      stack.pop();
      continue;
    }
    const node = toNode(tag, parseAttrs(rest));
    (stack.at(-1) as PieceNode).children.push(node);
    if (!selfClosing) stack.push(node);
  }
  if (!sawSvg || stack.length !== 1) throw new Error("Malformed piece SVG");
  return root.children;
}

/** Builds the whole set from file name to SVG text. */
export function buildPieces(sources: Record<string, string>): PieceSet {
  const set = {} as PieceSet;
  for (const { code, file } of PIECE_FILES) {
    const svg = sources[file];
    if (svg === undefined) throw new Error(`Missing ${file}`);
    set[code] = parsePiece(svg);
  }
  return set;
}
