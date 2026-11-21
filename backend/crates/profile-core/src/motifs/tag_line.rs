//! Tagging a line with motifs (R2 Task 5).

use std::collections::BTreeSet;

use chess_core::Color;

use super::rules::{self, LineContext};
use crate::board::{play_line, settled_plies, settled_swing};
use crate::types::{LineAnalysis, MotifTag};

/// Recapture-chain length used for the swing (TS M2 Task 3 default `settleExtra`).
pub const SETTLE_EXTRA: usize = 4;

/// Longest PV prefix the tagger replays; later moves are ignored (bounds the work per line).
pub const MAX_REPLAY_PLIES: usize = 128;

/// The swing of `pv` for `pov` (Task 4's `settled_swing`, `max_plies` usually 8) plus its motifs.
///
/// No swing, no tags: a line whose swing has no `depth` gets an empty tag list. Otherwise the
/// rules look only at plies `≤ depth + 1`, except the mate rules, which use the whole PV (the
/// mate is detected there, and `depth` may be set by an earlier material swing). The whole PV is
/// replayed, because `depth` may exceed `max_plies` for a deep mate. Tags come in `MotifTag`
/// declaration order without duplicates. Never panics; invalid input gives no tags. Only the
/// first [`MAX_REPLAY_PLIES`] moves of `pv` are read.
pub fn tag_line(fen: &str, pv: &[String], pov: Color, max_plies: usize) -> LineAnalysis {
    let pv = pv.get(..MAX_REPLAY_PLIES).unwrap_or(pv);
    let swing = settled_swing(fen, pv, pov, max_plies, SETTLE_EXTRA);
    let Some(depth) = swing.depth else {
        return LineAnalysis {
            swing,
            tags: Vec::new(),
        };
    };
    let steps = play_line(fen, pv, None);
    let settled = settled_plies(fen, pv, pov, max_plies, SETTLE_EXTRA);
    let window = usize::try_from(depth)
        .unwrap_or(usize::MAX)
        .saturating_add(1)
        .min(steps.len());
    let settled_limit = max_plies
        .saturating_add(SETTLE_EXTRA)
        .saturating_add(1)
        .max(window)
        .min(steps.len());
    let ctx = LineContext {
        pov,
        swing: &swing,
        steps: &steps,
        window,
        settled: &settled,
        settled_limit,
    };

    let mut tags = BTreeSet::new();
    let flags = [
        (MotifTag::HangingPiece, rules::hanging_piece(&ctx)),
        (MotifTag::Fork, rules::fork(&ctx)),
        (MotifTag::Pin, rules::pin(&ctx)),
        (MotifTag::Skewer, rules::skewer(&ctx)),
        (MotifTag::DiscoveredAttack, rules::discovered_attack(&ctx)),
        (MotifTag::Deflection, rules::deflection(&ctx)),
        (MotifTag::Sacrifice, rules::sacrifice(&ctx)),
        (MotifTag::Promotion, rules::promotion(&ctx)),
        (MotifTag::BackRankMate, rules::back_rank_mate(&ctx)),
        (MotifTag::SmotheredMate, rules::smothered_mate(&ctx)),
    ];
    tags.extend(flags.into_iter().filter(|(_, on)| *on).map(|(t, _)| t));
    tags.extend(rules::mate(&ctx));
    LineAnalysis {
        swing,
        tags: tags.into_iter().collect(),
    }
}
