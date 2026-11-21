// Seeded hand-drawn variation (DESIGN.md, "Pen stroke geometry", "Determinism"). The seed is the
// FNV-1a 32-bit hash of a key made from the mark and its squares (`ring:b5`, `arrow:c3b5`), fed
// into mulberry32, so the same mark looks the same on every device, visit and screenshot.

import { formatPath, type PathCommand, parsePath } from "./path.js";

/** FNV-1a 32-bit: offset basis 2166136261, prime 16777619, with `Math.imul`. */
export function fnv1a(key: string): number {
  let h = 2166136261;
  for (let i = 0; i < key.length; i++) h = Math.imul(h ^ key.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** mulberry32: a 32-bit seeded generator of numbers in [0, 1). */
export function mulberry32(seed: number): () => number {
  let a = seed | 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), a | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const seededRandom = (key: string) => mulberry32(fnv1a(key));

/**
 * Bends each straight segment of `d` into a quadratic that bows 2–4% of its length to one side
 * (amount and side seeded, two draws per segment in order), with its endpoints kept. `amount`
 * scales the bow: 0 leaves the path straight. Curves already in `d` are kept as they are.
 * The same input always gives the same output.
 */
export function roughen(d: string, seed: string, amount = 1): string {
  return formatPath(roughenCommands(parsePath(d), seededRandom(seed), amount));
}

/** `roughen` on parsed commands, drawing from a generator the caller may go on using. */
export function roughenCommands(
  cmds: readonly PathCommand[],
  rand: () => number,
  amount = 1,
): PathCommand[] {
  const out: PathCommand[] = [];
  let at = { x: 0, y: 0 };
  for (const c of cmds) {
    if (c.type === "L") {
      const bow = (0.02 + 0.02 * rand()) * amount;
      const side = rand() < 0.5 ? -1 : 1;
      const dx = c.x - at.x;
      const dy = c.y - at.y;
      // The curve's midpoint sits half the control point's offset from the chord, so the control
      // point goes out twice the bow. The normal is (−dy, dx) over the length, times the length.
      const k = 2 * bow * side;
      out.push({
        type: "Q",
        cx: (at.x + c.x) / 2 - dy * k,
        cy: (at.y + c.y) / 2 + dx * k,
        x: c.x,
        y: c.y,
      });
    } else out.push(c);
    if (c.type !== "Z") at = { x: c.x, y: c.y };
  }
  return out;
}
