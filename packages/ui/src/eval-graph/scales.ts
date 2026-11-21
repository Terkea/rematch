// Pure geometry for the eval graph, kept apart so it can be tested without rendering.
import { Scale } from "@visx/visx";

/** x of a ply (1-based) across `width`, the first ply at 0 and the last at `width`. */
export function xForPly(ply: number, width: number, count: number): number {
  if (count <= 1) return width / 2;
  return Scale.scaleLinear<number>({ domain: [1, count], range: [0, width], clamp: true })(ply);
}

/** y of White's win percent (0 to 100) in a plot `height` tall, White winning at the top. */
export function yForWin(win: number, height: number): number {
  return Scale.scaleLinear<number>({ domain: [0, 100], range: [height, 0], clamp: true })(win);
}

/** The ply nearest to `x`, clamped to 1..count. */
export function plyForX(x: number, width: number, count: number): number {
  if (count <= 1 || width <= 0) return 1;
  const ply = Math.round((x / width) * (count - 1)) + 1;
  return Math.min(count, Math.max(1, ply));
}
