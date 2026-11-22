import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { crc32, deflateSync } from "node:zlib";
import { brand } from "../src/brand";

// The favicon (M1): an `ink` tile holding a 2x2 board in `board-light` and `board-dark`, the
// product's one picture of itself. Every colour comes from the day tokens in tokens.css, so the
// mark follows a token change when this is run again:
//
//   pnpm --filter @ci/web exec tsx scripts/favicon.ts
//
// It writes `src/app/favicon.ico` (16, 32 and 48 px PNGs) and `src/app/icon.svg`, which Next
// serves at `/favicon.ico` and links from every page. A test checks both match the tokens.

export type MarkColours = { tile: string; light: string; dark: string };

/** The `--color-*` values of the first (day) block of tokens.css. */
export function readDayTokens(css: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of css.matchAll(/--color-([a-z-]+):\s*(#[0-9a-f]{6})\s*;/gi)) {
    const [, name = "", value = ""] = m;
    if (!(name in out)) out[name] = value.toLowerCase();
  }
  return out;
}

export function markColours(tokens: Record<string, string>): MarkColours {
  const pick = (name: string) => {
    const v = tokens[name];
    if (!v) throw new Error(`favicon: no --color-${name} in tokens.css`);
    return v;
  };
  return { tile: pick("ink"), light: pick("board-light"), dark: pick("board-dark") };
}

// The geometry on a unit square: a rounded tile, and the board inset inside it.
const RADIUS = 0.22;
const INSET = 0.2;

type Rgba = [number, number, number, number];

const rgb = (hex: string): Rgba => [
  Number.parseInt(hex.slice(1, 3), 16),
  Number.parseInt(hex.slice(3, 5), 16),
  Number.parseInt(hex.slice(5, 7), 16),
  255,
];

function insideTile(x: number, y: number): boolean {
  const cx = Math.min(Math.max(x, RADIUS), 1 - RADIUS);
  const cy = Math.min(Math.max(y, RADIUS), 1 - RADIUS);
  return (x - cx) ** 2 + (y - cy) ** 2 <= RADIUS ** 2;
}

/** The mark's colour at a point of the unit square, or transparent outside the tile. */
function sample(x: number, y: number, c: MarkColours): Rgba {
  if (!insideTile(x, y)) return [0, 0, 0, 0];
  if (x >= INSET && x < 1 - INSET && y >= INSET && y < 1 - INSET) {
    const col = x < 0.5 ? 0 : 1;
    const row = y < 0.5 ? 0 : 1;
    return rgb((col + row) % 2 === 0 ? c.light : c.dark);
  }
  return rgb(c.tile);
}

/** RGBA pixels at `size`, 4x4 supersampled for the corners. */
function pixels(size: number, c: MarkColours): Buffer {
  const SS = 4;
  const out = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const [sr, sg, sb, sa] = sample(
            (px + (sx + 0.5) / SS) / size,
            (py + (sy + 0.5) / SS) / size,
            c,
          );
          r += sr * sa;
          g += sg * sa;
          b += sb * sa;
          a += sa;
        }
      }
      const i = (py * size + px) * 4;
      out[i] = a ? Math.round(r / a) : 0;
      out[i + 1] = a ? Math.round(g / a) : 0;
      out[i + 2] = a ? Math.round(b / a) : 0;
      out[i + 3] = Math.round(a / (SS * SS));
    }
  }
  return out;
}

function chunk(type: string, data: Buffer): Buffer {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])) >>> 0, 0);
  return Buffer.concat([head, data, crc]);
}

function png(size: number, c: MarkColours): Buffer {
  const rgba = pixels(size, c);
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(6, 9); // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const SIZES = [16, 32, 48] as const;

/** An ICO file holding one PNG per size. */
export function faviconIco(c: MarkColours): Buffer {
  const images = SIZES.map((s) => png(s, c));
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach((img, i) => {
    const at = 6 + i * 16;
    header.writeUInt8(SIZES[i] ?? 0, at);
    header.writeUInt8(SIZES[i] ?? 0, at + 1);
    header.writeUInt16LE(1, at + 4); // colour planes
    header.writeUInt16LE(32, at + 6); // bits per pixel
    header.writeUInt32LE(img.length, at + 8);
    header.writeUInt32LE(offset, at + 12);
    offset += img.length;
  });
  return Buffer.concat([header, ...images]);
}

/** The same mark as SVG, for browsers that take a vector icon. */
export function iconSvg(c: MarkColours): string {
  const u = (v: number) => Number((v * 32).toFixed(3));
  const cell = u(0.5 - INSET);
  const a = u(INSET);
  const b = u(0.5);
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">`,
    `<title>${brand.name}</title>`,
    `<rect width="32" height="32" rx="${u(RADIUS)}" fill="${c.tile}"/>`,
    `<rect x="${a}" y="${a}" width="${cell}" height="${cell}" fill="${c.light}"/>`,
    `<rect x="${b}" y="${a}" width="${cell}" height="${cell}" fill="${c.dark}"/>`,
    `<rect x="${a}" y="${b}" width="${cell}" height="${cell}" fill="${c.dark}"/>`,
    `<rect x="${b}" y="${b}" width="${cell}" height="${cell}" fill="${c.light}"/>`,
    `</svg>`,
    "",
  ].join("\n");
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const here = dirname(fileURLToPath(import.meta.url));
  const tokens = readFileSync(join(here, "../../../packages/ui/src/styles/tokens.css"), "utf8");
  const colours = markColours(readDayTokens(tokens));
  writeFileSync(join(here, "../src/app/favicon.ico"), faviconIco(colours));
  writeFileSync(join(here, "../src/app/icon.svg"), iconSvg(colours));
  console.log("favicon: wrote src/app/favicon.ico and src/app/icon.svg");
}
