//! Critical-ply selection (R2 Task 25, TS M2 Task 22 `selectCriticalPlies`).

use chess_core::{Color, MoveRecord};

/// A ply with at least this ΔW (win% lost by the mover) is critical.
pub const CRITICAL_DELTA_W: f64 = 5.0;
/// The ply after a swing of at least this ΔW is critical too (the reply to a big mistake).
pub const AFTER_SWING_DELTA_W: f64 = 10.0;

/// The plies of `colors` to probe with MultiPV: ΔW ≥ 5, or the previous ply's ΔW ≥ 10. Over
/// `max`, the highest ΔW are kept (ties: the lower ply). Ascending.
///
/// Known bias (ADR-R2-14): a critical position reached without a prior swing and played well is
/// not probed, so `is_critical`-based rates compare only with references built the same way.
pub fn select_critical_plies(records: &[MoveRecord], colors: &[Color], max: usize) -> Vec<u32> {
    let mut picked: Vec<(f64, u32)> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| colors.contains(&r.color))
        .filter(|(i, r)| {
            r.delta_w >= CRITICAL_DELTA_W
                || i.checked_sub(1)
                    .and_then(|p| records.get(p))
                    .is_some_and(|prev| prev.delta_w >= AFTER_SWING_DELTA_W)
        })
        .map(|(_, r)| (r.delta_w, r.ply))
        .collect();
    if picked.len() > max {
        picked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        picked.truncate(max);
    }
    let mut plies: Vec<u32> = picked.into_iter().map(|(_, p)| p).collect();
    plies.sort_unstable();
    plies
}
