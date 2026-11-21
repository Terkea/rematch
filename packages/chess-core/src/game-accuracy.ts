import type { Color } from "./pgn.js";
import { moveAccuracy } from "./score.js";

function clamp(x: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, x));
}

function stdev(xs: number[]): number {
  const mean = xs.reduce((a, b) => a + b, 0) / xs.length;
  return Math.sqrt(xs.reduce((a, b) => a + (b - mean) ** 2, 0) / xs.length);
}

/**
 * Lichess game accuracy for one colour: the mean of a volatility-weighted
 * mean and a harmonic mean of per-move accuracies.
 *
 * `winPercentsWhitePov[i]` is white's win% in the position before ply i + 1
 * (the last entry is the final position). Returns 100 if the colour made no moves.
 */
export function gameAccuracy(winPercentsWhitePov: number[], color: Color): number {
  const n = winPercentsWhitePov.length;
  const windowSize = clamp(Math.floor(n / 10), 2, 8);
  const accuracies: number[] = [];
  const weights: number[] = [];

  for (let i = 0; i + 1 < n; i++) {
    const isWhiteMove = i % 2 === 0;
    if (isWhiteMove !== (color === "white")) continue;
    const before = winPercentsWhitePov[i] as number;
    const after = winPercentsWhitePov[i + 1] as number;
    accuracies.push(
      color === "white" ? moveAccuracy(before, after) : moveAccuracy(100 - before, 100 - after),
    );
    // Like lichess, the first windowSize - 2 moves share the first window.
    const start = clamp(i - (windowSize - 2), 0, Math.max(0, n - windowSize));
    weights.push(clamp(stdev(winPercentsWhitePov.slice(start, start + windowSize)), 0.5, 12));
  }

  if (accuracies.length === 0) return 100;

  const weightSum = weights.reduce((a, b) => a + b, 0);
  const weighted = accuracies.reduce((sum, acc, i) => sum + acc * (weights[i] as number), 0);
  const weightedMean = weighted / weightSum;
  // A move with accuracy 0 drives the harmonic mean to its limit, 0.
  const harmonicMean = accuracies.some((a) => a <= 0)
    ? 0
    : accuracies.length / accuracies.reduce((sum, a) => sum + 1 / a, 0);
  return (weightedMean + harmonicMean) / 2;
}
