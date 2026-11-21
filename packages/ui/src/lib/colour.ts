// WCAG 2.x contrast for token colours. Used by the token tests and by /styleguide's live labels.

/** sRGB channels 0–255 and alpha 0–1. */
export type Rgba = readonly [number, number, number, number];

/**
 * Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, and `rgb()`/`rgba()` in space or comma syntax
 * (alpha as a number or a percentage). Anything else returns undefined.
 */
export function parseColour(value: string): Rgba | undefined {
  const v = value.trim();
  const hex = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(v)?.[1];
  if (hex) {
    const full = hex.length <= 4 ? [...hex].map((c) => c + c).join("") : hex;
    const byte = (i: number) => Number.parseInt(full.slice(i * 2, i * 2 + 2), 16);
    return [byte(0), byte(1), byte(2), full.length === 8 ? byte(3) / 255 : 1];
  }
  const fn =
    /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)\s*(?:[/,]\s*([\d.]+)(%?))?\s*\)$/i.exec(v);
  if (fn) {
    const alpha = fn[4] === undefined ? 1 : Number(fn[4]) / (fn[5] ? 100 : 1);
    return [Number(fn[1]), Number(fn[2]), Number(fn[3]), alpha];
  }
  return undefined;
}

/** Source-over compositing of `top` on an opaque `bottom`, rounded to 8 bits as browsers render. */
export function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top[3];
  const mix = (i: 0 | 1 | 2) => Math.round(top[i] * a + bottom[i] * (1 - a));
  return [mix(0), mix(1), mix(2), 1];
}

/** WCAG relative luminance. */
export function luminance([r, g, b]: Rgba): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Contrast ratio of `fg` on an opaque `bg`. A translucent `fg` is composited over `bg` first. */
export function contrast(fg: Rgba, bg: Rgba): number {
  const a = luminance(over(fg, bg));
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
