/**
 * A token's computed value as the foundations table prints it. A production build's CSS minifier
 * rewrites a translucent `rgb(r g b / a)` as an 8-digit hex; this prints every translucent hex
 * (8 or 4 digits) back in the `rgb(r g b / a)` form tokens.css uses, alpha to two places, so dev
 * and production show the same text. Opaque hex and everything else pass through.
 */
export function tokenValue(value: string): string {
  return value.replace(/#([0-9a-f]{8}|[0-9a-f]{4})\b/gi, (_, hex: string) => {
    const full = hex.length === 4 ? [...hex].map((c) => c + c).join("") : hex;
    const [r, g, b, a] = [0, 2, 4, 6].map((i) => Number.parseInt(full.slice(i, i + 2), 16));
    const alpha = Number(((a as number) / 255).toFixed(2));
    return `rgb(${r} ${g} ${b} / ${alpha})`;
  });
}
