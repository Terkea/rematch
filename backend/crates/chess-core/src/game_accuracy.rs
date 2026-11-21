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
    let w = win_percents_white_pov;
    let n = w.len();
    let window = (n / 10).clamp(2, 8);
    let mut accuracies: Vec<f64> = Vec::new();
    let mut weights: Vec<f64> = Vec::new();

    for i in 0..n.saturating_sub(1) {
        let is_white_move = i % 2 == 0;
        if is_white_move != (color == Color::White) {
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
        return 100.0;
    }

    let weight_sum = weights.iter().fold(0.0, |a, b| a + b);
    let weighted = accuracies
        .iter()
        .zip(&weights)
        .fold(0.0, |sum, (acc, wt)| sum + acc * wt);
    let weighted_mean = weighted / weight_sum;
    // A move with accuracy 0 drives the harmonic mean to its limit, 0.
    let harmonic_mean = if accuracies.iter().any(|a| *a <= 0.0) {
        0.0
    } else {
        accuracies.len() as f64 / accuracies.iter().fold(0.0, |sum, a| sum + 1.0 / a)
    };
    (weighted_mean + harmonic_mean) / 2.0
}
