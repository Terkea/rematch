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
  return gameAccuracyFrom(winPercentsWhitePov, "white", color) ?? 100;
}

/**
 * `gameAccuracy` of a series whose first move is `start`'s (a phase slice, ADR-FIX-C-1), or
 * null when `color` made no move in it.
 *
 * A port of lila `AccuracyPercent.gameAccuracy`
 * (lichess-org/lila `modules/analyse/src/main/AccuracyPercent.scala`):
 *
 *     val windowSize = ((cps.size - 1) / 10).squeeze(2, 8)
 *     val windows = List.fill(windowSize.atMost(allWinPercents.size) - 2)(allWinPercents.take(windowSize))
 *       ::: allWinPercents.sliding(windowSize).toList
 *     val weights = windows.map(_ ... Maths.standardDeviation(...).so(_.squeeze(0.5, 12)))
 *     ... yield AccuracyPercent((weighted + harmonic) / 2)
 *
 * `cps` holds the initial position too, so the window comes from the number of moves. The
 * harmonic half is scalalib `Maths.harmonicMean`, which floors every value at 1:
 *
 *     a.size / a.foldLeft(0d) { (acc, v) => acc + 1 / Math.max(1, v) }
 */
export function gameAccuracyFrom(
  winPercentsWhitePov: number[],
  start: Color,
  color: Color,
): number | null {
  const n = winPercentsWhitePov.length;
  const moves = Math.max(0, n - 1);
  const windowSize = clamp(Math.floor(moves / 10), 2, 8);
  const accuracies: number[] = [];
  const weights: number[] = [];

  for (let i = 0; i < moves; i++) {
    const moverIsStart = i % 2 === 0;
    const mover: Color = moverIsStart === (start === "white") ? "white" : "black";
    if (mover !== color) continue;
    const before = winPercentsWhitePov[i] as number;
    const after = winPercentsWhitePov[i + 1] as number;
    accuracies.push(
      color === "white" ? moveAccuracy(before, after) : moveAccuracy(100 - before, 100 - after),
    );
    // Like lichess, the first windowSize - 2 moves share the first window.
    const first = clamp(i - (windowSize - 2), 0, Math.max(0, n - windowSize));
    weights.push(clamp(stdev(winPercentsWhitePov.slice(first, first + windowSize)), 0.5, 12));
  }

  if (accuracies.length === 0) return null;

  const weightSum = weights.reduce((a, b) => a + b, 0);
  const weighted = accuracies.reduce((sum, acc, i) => sum + acc * (weights[i] as number), 0);
  const weightedMean = weighted / weightSum;
  const harmonicMean =
    accuracies.length / accuracies.reduce((sum, a) => sum + 1 / Math.max(1, a), 0);
  return (weightedMean + harmonicMean) / 2;
}
