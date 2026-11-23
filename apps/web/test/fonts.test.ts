import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { brotliDecompressSync } from "node:zlib";
import { describe, expect, it } from "vitest";

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const read = (path: string) => readFileSync(here(path));

// WOFF2 table tags by their index in the directory's flags byte (WOFF2 spec, section 5.1).
const knownTags =
  `cmap head hhea hmtx maxp name OS/2 post cvt  fpgm glyf loca prep CFF  VORG EBDT EBLC gasp
hdmx kern LTSH PCLT VDMX vhea vmtx BASE GDEF GPOS GSUB EBSC JSTF MATH CBDT CBLC COLR CPAL SVG  sbix acnt
avar bdat bloc bsln cvar fdsc feat fmtx fvar gvar hsty just lcar mort morx opbd prop trak Zapf Silf Glat
Gloc Feat Sill`
    .replace(/\n/g, " ")
    .match(/.{4}\s?/g)
    ?.map((tag) => tag.slice(0, 4)) as string[];

interface Axis {
  tag: string;
  min: number;
  default: number;
  max: number;
}

/** Reads the variation axes from a WOFF2 file's fvar table. */
function woff2Axes(file: Buffer): Axis[] {
  expect(file.toString("latin1", 0, 4)).toBe("wOF2");
  const numTables = file.readUInt16BE(12);
  const compressedSize = file.readUInt32BE(20);
  let at = 48;
  const base128 = () => {
    let value = 0;
    for (let i = 0; i < 5; i++) {
      const byte = file.readUInt8(at++);
      value = value * 128 + (byte & 0x7f);
      if ((byte & 0x80) === 0) return value;
    }
    throw new Error("bad UIntBase128");
  };
  const tables: { tag: string; length: number }[] = [];
  for (let i = 0; i < numTables; i++) {
    const flags = file.readUInt8(at++);
    const index = flags & 0x3f;
    const version = flags >> 6;
    let tag = knownTags[index] ?? "";
    if (index === 63) {
      tag = file.toString("latin1", at, at + 4);
      at += 4;
    }
    const origLength = base128();
    const transformed =
      tag === "glyf" || tag === "loca" ? version === 0 : tag === "hmtx" ? version === 1 : false;
    tables.push({ tag, length: transformed ? base128() : origLength });
  }
  const stream = brotliDecompressSync(file.subarray(at, at + compressedSize));
  let offset = 0;
  for (const table of tables) {
    if (table.tag === "fvar") {
      const fvar = stream.subarray(offset, offset + table.length);
      const axesOffset = fvar.readUInt16BE(4);
      const count = fvar.readUInt16BE(8);
      const size = fvar.readUInt16BE(10);
      return Array.from({ length: count }, (_, i) => {
        const r = axesOffset + i * size;
        return {
          tag: fvar.toString("latin1", r, r + 4),
          min: fvar.readInt32BE(r + 4) / 65536,
          default: fvar.readInt32BE(r + 8) / 65536,
          max: fvar.readInt32BE(r + 12) / 65536,
        };
      });
    }
    offset += table.length;
  }
  return [];
}

// DESIGN.md's type scale: step, size/line height, Literata opsz.
const typeRows = (() => {
  const md = read("../../../packages/ui/DESIGN.md").toString("utf8").split(/\r?\n/);
  const start = md.findIndex((line) => line.startsWith("### Scale and optical size"));
  const rows: { step: string; size: number; opsz: number }[] = [];
  for (const line of md.slice(start)) {
    const match = /^\| `([\w]+)` \| (\d+)\/\d+ \| (\d+) \|/.exec(line);
    if (match) rows.push({ step: match[1] ?? "", size: Number(match[2]), opsz: Number(match[3]) });
    else if (rows.length > 0 && !line.startsWith("|")) break;
  }
  return rows;
})();

describe("self-hosted fonts", () => {
  it("reads all eleven type steps from DESIGN.md", () => {
    expect(typeRows).toHaveLength(11);
  });

  for (const file of ["Literata-VF.woff2", "Literata-Italic-VF.woff2"]) {
    it(`${file} is variable in opsz and wght, and covers every opsz in the type scale`, () => {
      const axes = woff2Axes(read(`../src/app/fonts/${file}`));
      const opsz = axes.find((axis) => axis.tag === "opsz");
      const wght = axes.find((axis) => axis.tag === "wght");
      expect(opsz).toBeDefined();
      expect(wght?.min).toBeLessThanOrEqual(400);
      expect(wght?.max).toBeGreaterThanOrEqual(600);
      for (const row of typeRows) {
        expect(row.opsz).toBeGreaterThanOrEqual(opsz?.min ?? Number.NaN);
        expect(row.opsz).toBeLessThanOrEqual(opsz?.max ?? Number.NaN);
      }
    });
  }

  it("SchibstedGrotesk-VF.woff2 covers weights 400 to 700", () => {
    const wght = woff2Axes(read("../src/app/fonts/SchibstedGrotesk-VF.woff2")).find(
      (axis) => axis.tag === "wght",
    );
    expect(wght?.min).toBeLessThanOrEqual(400);
    expect(wght?.max).toBeGreaterThanOrEqual(700);
  });

  it("sets opsz from the rendered size, which is DESIGN.md's mapping", () => {
    // font-optical-sizing: auto sets opsz to the px size, so the table's opsz must equal its size.
    expect(read("../src/app/globals.css").toString("utf8")).toMatch(/font-optical-sizing:\s*auto;/);
    for (const row of typeRows) expect(row.opsz, row.step).toBe(row.size);
  });

  it("Caveat-VF.woff2 (dad's handwriting) covers weights 400 to 700", () => {
    const wght = woff2Axes(read("../src/app/fonts/Caveat-VF.woff2")).find(
      (axis) => axis.tag === "wght",
    );
    expect(wght?.min).toBeLessThanOrEqual(400);
    expect(wght?.max).toBeGreaterThanOrEqual(700);
  });

  it("loads Caveat as --font-caveat, not preloaded", () => {
    const layout = read("../src/app/layout.tsx").toString("utf8");
    const hand = /localFont\(\{[^}]*Caveat-VF\.woff2[\s\S]*?\n\}\);/.exec(layout)?.[0];
    expect(hand).toBeDefined();
    expect(hand).toMatch(/preload: false/);
    expect(hand).toMatch(/variable: "--font-caveat"/);
  });

  it("loads Literata italic as its own family, not preloaded", () => {
    const layout = read("../src/app/layout.tsx").toString("utf8");
    const italic = /localFont\(\{[^}]*Literata-Italic-VF\.woff2[\s\S]*?\n\}\);/.exec(layout)?.[0];
    expect(italic).toBeDefined();
    expect(italic).toMatch(/preload: false/);
    expect(italic).toMatch(/variable: "--font-literata-italic"/);
  });
});
