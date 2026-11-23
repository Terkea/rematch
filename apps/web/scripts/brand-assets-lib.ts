import { brand } from "../src/brand";

// The pure half of `scripts/brand-assets.ts`: text transforms of the committed logo SVGs in
// packages/ui/assets/brand, and the ICO container. The rasters come from the system Chrome.

/** Ink, by day (the spec's `--color-ink`). The logo is ink only, never pen red. */
export const BRAND_INK = "#1F2A3D";
/** Ink at night (the spec's night cream), for dark browser chrome. */
export const BRAND_NIGHT_INK = "#E8E2D6";
/** Pencil, the warm graphite of secondary text on the social card. */
export const BRAND_PENCIL = "#6B6458";
/** Pencil at night (the warm desk-lamp night's `--color-pencil`), for the README's dark lockup. */
export const BRAND_NIGHT_PENCIL = "#ADA394";
/** The paper behind the raster icons, where a platform needs an opaque tile. */
export const BRAND_PAPER = "#F4EFE6";

const WORD = "font-family:'Literata Italic';font-style:italic;font-optical-sizing:auto";

/**
 * The wordmark with its tagline (the owner's pick, 2026-11-23): "Rematch" in Literata italic with
 * a capital R, and "chess improver" a third of its size in Literata italic, in pencil, set flush
 * with the end of the word like a signature under a title. `size` is the wordmark's font size.
 */
function wordmarkBlock(size: number, sub: string): string {
  return `<div style="display:flex;flex-direction:column;align-items:flex-end">
  <p style="margin:0;${WORD};font-weight:500;font-size:${size}px;line-height:1;letter-spacing:-0.007em">Rematch</p>
  <p style="margin:${Math.round(size * 0.02)}px ${Math.round(size * 0.02)}px 0 0;${WORD};font-weight:400;font-size:${Math.round(size * 0.3)}px;line-height:1;color:${sub}">chess improver</p>
</div>`;
}

/**
 * The README lockup on a transparent ground: the mark, then the wordmark and its tagline. It is
 * laid out at 640 x 220; `zoom` draws it smaller (the sign-in email's 220 x 76).
 */
export function lockupHtml(
  mark: string,
  faces: string,
  ink: string,
  sub: string,
  zoom = 1,
): string {
  return `<style>${faces}</style>
<body style="margin:0;width:640px;height:220px;zoom:${zoom};background:transparent;color:${ink};display:flex;align-items:center;gap:30px">
${mark.replace("<svg", '<svg height="208" style="display:block;flex:none"')}
${wordmarkBlock(96, sub)}
</body>`;
}

/** The social card: the full mark beside the lockup and the promise. */
export function cardHtml(mark: string, faces: string, w: number, h: number): string {
  return `<style>${faces}</style>
<body style="margin:0;width:${w}px;height:${h}px;background:${BRAND_PAPER};color:${BRAND_INK};display:flex;align-items:center;justify-content:center;gap:${Math.round(w * 0.05)}px">
${mark.replace("<svg", `<svg height="${Math.round(h * 0.74)}" style="display:block;flex:none"`)}
<div style="display:flex;flex-direction:column;align-items:flex-start">
${wordmarkBlock(Math.round(h * 0.25), BRAND_PENCIL)}
  <p style="margin:${Math.round(h * 0.075)}px 0 0 ${Math.round(h * 0.008)}px;font-family:'Literata';font-weight:400;font-size:${Math.round(h * 0.07)}px">Stop losing to the same move.</p>
</div></body>`;
}

/** The SVG's viewBox and its inner markup, without the `<title>`. */
export function svgParts(source: string): { viewBox: string; body: string } {
  const viewBox = source.match(/viewBox="([^"]+)"/)?.[1];
  const body = source.match(/<svg[^>]*>([\s\S]*)<\/svg>/)?.[1];
  if (!viewBox || body === undefined) throw new Error("brand: not an SVG");
  return { viewBox, body: body.replace(/<title>[\s\S]*?<\/title>/, "").trim() };
}

/** The browser-tab icon: the icon in ink, and in night cream when the browser chrome is dark. */
export function iconSvg(source: string): string {
  const { viewBox, body } = svgParts(source);
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}">`,
    `<title>${brand.name}</title>`,
    `<style>svg{color:${BRAND_INK}}@media (prefers-color-scheme: dark){svg{color:${BRAND_NIGHT_INK}}}</style>`,
    body,
    "</svg>",
    "",
  ].join("\n");
}

/** The mark in one fixed colour, for an image file (the docs header), where currentColor is black. */
export function solidSvg(source: string, colour: string): string {
  const { viewBox, body } = svgParts(source);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}"><title>Rematch</title>${body.replaceAll("currentColor", colour)}</svg>\n`;
}

/** Prefix every `id` and its `url(#…)` references, so the two marks can't collide. */
function scopeIds(body: string, prefix: string): string {
  return body
    .replace(/id="([^"]+)"/g, `id="${prefix}-$1"`)
    .replace(/url\(#([^)]+)\)/g, `url(#${prefix}-$1)`);
}

/**
 * The TypeScript module that inlines the marks into the app: `mid` (rematch-mark-mid.svg, drawn
 * for 40-64 px: headers and the footer) and `full` (rematch-mark.svg, 96 px and up).
 */
export function marksModule(mid: string, full: string): string {
  const entry = (source: string, prefix: string) => {
    const { viewBox, body } = svgParts(source);
    if (/#[0-9a-f]{3,6}(?![0-9a-z])|<mask/i.test(body))
      throw new Error("brand: a mark has a colour of its own");
    return `{ viewBox: ${JSON.stringify(viewBox)}, body: ${JSON.stringify(scopeIds(body, prefix))} }`;
  };
  return `// Generated by scripts/brand-assets.ts from packages/ui/assets/brand/rematch-mark-mid.svg and
// rematch-mark.svg. Do not edit by hand: run \`pnpm --filter @ci/web brand\`.
export const REMATCH_MARKS = {
  mid: ${entry(mid, "rematch-mid")},
  full: ${entry(full, "rematch-full")},
} as const;
`;
}

/** An ICO file holding one PNG per size (Vista and later read PNG entries). */
export function icoFile(images: { size: number; png: Buffer }[]): Buffer {
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(({ size, png }, i) => {
    const at = 6 + i * 16;
    header.writeUInt8(size >= 256 ? 0 : size, at);
    header.writeUInt8(size >= 256 ? 0 : size, at + 1);
    header.writeUInt16LE(1, at + 4);
    header.writeUInt16LE(32, at + 6);
    header.writeUInt32LE(png.length, at + 8);
    header.writeUInt32LE(offset, at + 12);
    offset += png.length;
  });
  return Buffer.concat([header, ...images.map((i) => i.png)]);
}
