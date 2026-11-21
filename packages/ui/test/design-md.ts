// Readers for the two token sources: DESIGN.md (the spec, which wins) and tokens.css (the
// implementation). The token tests compare both, and the JS motion tokens, against each other, so a
// value changed in one place and not the others fails a test (ADR-WEB-7).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export const designMd = readFileSync(here("../DESIGN.md"), "utf8");
export const tokensCss = readFileSync(here("../src/styles/tokens.css"), "utf8");

/** The first markdown table after the line containing `marker`, as rows of trimmed cells. */
export function tableAfter(marker: string, source = designMd): string[][] {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => line.includes(marker));
  if (start < 0) throw new Error(`DESIGN.md has no line containing "${marker}"`);
  let i = start + 1;
  while (i < lines.length && !lines[i]?.startsWith("|")) i++;
  const rows: string[][] = [];
  for (; i < lines.length && lines[i]?.startsWith("|"); i++) {
    rows.push(
      (lines[i] ?? "")
        .slice(1, -1)
        .split("|")
        .map((cell) => cell.trim()),
    );
  }
  // Drop the header row and the |---| separator.
  return rows.slice(2);
}

/** The first `code` span in a cell, or the cell itself. */
export function code(cell: string | undefined): string {
  const match = /`([^`]+)`/.exec(cell ?? "");
  return (match?.[1] ?? cell ?? "").trim();
}

export function ms(cell: string | undefined): number {
  const match = /^(\d+(?:\.\d+)?)ms$/.exec(code(cell));
  if (!match) throw new Error(`not a ms value: ${cell}`);
  return Number(match[1]);
}

export function px(value: string): number {
  const match = /^(-?\d+(?:\.\d+)?)(px|rem)$/.exec(value.trim());
  if (!match) throw new Error(`not a px or rem value: ${value}`);
  return match[2] === "rem" ? Number(match[1]) * 16 : Number(match[1]);
}

export function bezier(value: string): number[] {
  const match = /^cubic-bezier\(([^)]+)\)$/.exec(value.trim());
  if (!match) throw new Error(`not a cubic-bezier: ${value}`);
  return (match[1] ?? "").split(",").map((n) => Number(n.trim()));
}

export type Declarations = Map<string, string>;

function declarations(body: string): Declarations {
  const out: Declarations = new Map();
  const withoutComments = body.replace(/\/\*[\s\S]*?\*\//g, "");
  for (const match of withoutComments.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name && value && value.trim() !== "initial") out.set(name, value.trim());
  }
  return out;
}

/** The body of the first `{ ... }` block whose selector (text before `{`) matches. */
function blockBody(selector: RegExp, source: string): string | undefined {
  const re = new RegExp(`${selector.source}\\s*\\{`, "g");
  const match = re.exec(source);
  if (!match) return undefined;
  let depth = 1;
  let i = match.index + match[0].length;
  const start = i;
  for (; i < source.length && depth > 0; i++) {
    if (source[i] === "{") depth++;
    else if (source[i] === "}") depth--;
  }
  return source.slice(start, i - 1);
}

export const css = {
  day: declarations(blockBody(/@theme static/, tokensCss) ?? ""),
  night: declarations(blockBody(/\[data-theme="dark"\]/, tokensCss) ?? ""),
  dayScope: declarations(blockBody(/\[data-theme="light"\]/, tokensCss) ?? ""),
  /** Values set in `@media (width >= Npx) { :root { ... } }` blocks, keyed by the min width. */
  media(): Map<number, Declarations> {
    const out = new Map<number, Declarations>();
    for (const match of tokensCss.matchAll(/@media \(width >= (\d+)px\)/g)) {
      const body = blockBody(
        new RegExp(`@media \\(width >= ${match[1]}px\\)`),
        tokensCss.slice(match.index),
      );
      out.set(Number(match[1]), declarations(body ?? ""));
    }
    return out;
  },
};

// WCAG 2.x contrast, with alpha colours composited over their background first.
export type Rgba = [number, number, number, number];

export function parseColour(value: string): Rgba {
  const v = value.trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(v);
  if (hex?.[1]) {
    const n = Number.parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*(?:\/\s*([\d.]+))?\s*\)$/.exec(v);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), Number(rgb[4] ?? 1)];
  throw new Error(`unparseable colour: ${value}`);
}

/** Source-over compositing, rounded to 8 bits per channel as the browser renders it. */
export function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top[3];
  const mix = (i: 0 | 1 | 2) => Math.round(top[i] * a + bottom[i] * (1 - a));
  return [mix(0), mix(1), mix(2), 1];
}

function luminance([r, g, b]: Rgba): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(fg: Rgba, bg: Rgba): number {
  const a = luminance(over(fg, bg));
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
