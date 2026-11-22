use crate::Color;
use crate::score::move_accuracy;

fn stdev(xs: &[f64]) -> f64 {
    let len = xs.len() as f64;
    let mean = xs.iter().fold(0.0, |a, b| a + b) / len;
    let var = xs.iter().fold(0.0, |a, b| a + (b - mean) * (b - mean)) / len;
    var.sqrt()
}

/// Lichess game accuracy for one colour: the mean of a volatility-weighted mean and a
/// harmonic mean of per-move accuracies.
///
/// `win_percents_white_pov[i]` is white's win% in the position before ply i + 1 (the last
/// entry is the final position). Returns 100 if the colour made no moves.
pub fn game_accuracy(win_percents_white_pov: &[f64], color: Color) -> f64 {
    game_accuracy_from(win_percents_white_pov, Color::White, color).unwrap_or(100.0)
}

/// [`game_accuracy`] of a series whose first move is `start`'s (a phase slice, ADR-FIX-C-1), or
/// `None` when `color` made no move in it.
///
/// A port of lila `AccuracyPercent.gameAccuracy`
/// (lichess-org/lila `modules/analyse/src/main/AccuracyPercent.scala`):
///
/// ```text
/// val windowSize = ((cps.size - 1) / 10).squeeze(2, 8)
/// val windows = List.fill(windowSize.atMost(allWinPercents.size) - 2)(allWinPercents.take(windowSize))
///   ::: allWinPercents.sliding(windowSize).toList
/// val weights = windows.map(_ ... Maths.standardDeviation(...).so(_.squeeze(0.5, 12)))
/// ... yield AccuracyPercent((weighted + harmonic) / 2)
/// ```
///
/// `cps` holds the initial position too, so the window comes from the number of moves. The
/// harmonic half is scalalib `Maths.harmonicMean`, which floors every value at 1:
///
/// ```text
/// a.size / a.foldLeft(0d) { (acc, v) => acc + 1 / Math.max(1, v) }
/// ```
pub fn game_accuracy_from(
    win_percents_white_pov: &[f64],
    start: Color,
    color: Color,
) -> Option<f64> {
    let w = win_percents_white_pov;
    let n = w.len();
    let moves = n.saturating_sub(1);
    let window = (moves / 10).clamp(2, 8);
    let mut accuracies: Vec<f64> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();

    for i in 0..moves {
        let mover_is_start = i % 2 == 0;
        let mover = if mover_is_start == (start == Color::White) {
            Color::White
        } else {
            Color::Black
        };
        if mover != color {
            continue;
        }
        let (before, after) = (w[i], w[i + 1]);
        accuracies.push(if color == Color::White {
            move_accuracy(before, after)
        } else {
            move_accuracy(100.0 - before, 100.0 - after)
        });
        // Like lichess, the first window - 2 moves share the first window.
        let start =
            (i as i64 - (window as i64 - 2)).clamp(0, n.saturating_sub(window) as i64) as usize;
        let end = (start + window).min(n);
        weights.push(stdev(&w[start..end]).clamp(0.5, 12.0));
    }

    if accuracies.is_empty() {
        return None;
    }

    let weight_sum = weights.iter().fold(0.0, |a, b| a + b);
    let weighted = accuracies
        .iter()
        .zip(&weights)
        .fold(0.0, |sum, (acc, wt)| sum + acc * wt);
    let weighted_mean = weighted / weight_sum;
    let harmonic_mean =
        accuracies.len() as f64 / accuracies.iter().fold(0.0, |sum, a| sum + 1.0 / a.max(1.0));
    Some((weighted_mean + harmonic_mean) / 2.0)
}
