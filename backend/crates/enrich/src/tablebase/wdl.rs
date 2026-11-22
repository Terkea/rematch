//! WDL from Stockfish output (R2 Task 23, ADR-R2-12).
//!
//! With `SyzygyPath` set and the root inside the tables, Stockfish 19 reports a tablebase win as
//! `cp 20000 - plies` (a loss as `cp -20000 + plies`), a draw as `cp 0`, and a 50-move-cursed win
//! or blessed loss as a small cp. A mate found by the search is reported as `mate n`.

use chess_core::EngineScore;
use engine::types::AnalysisLine;
use profile_core::types::Wdl;

/// Smallest |cp| read as a tablebase win or loss.
pub const TB_CP_THRESHOLD: i32 = 19_000;

/// WDL (side-to-move point of view) from the first line: `mate > 0` or `cp >= 19000` is 1,
/// `mate <= 0` or `cp <= -19000` is -1, any other cp is 0, and no lines is `None`.
pub fn wdl_from_lines(lines: &[AnalysisLine]) -> Option<Wdl> {
    let first = lines.iter().min_by_key(|l| l.multipv)?;
    Some(match first.score {
        EngineScore::Mate(m) if m > 0 => 1,
        EngineScore::Mate(_) => -1,
        EngineScore::Cp(cp) if cp >= TB_CP_THRESHOLD => 1,
        EngineScore::Cp(cp) if cp <= -TB_CP_THRESHOLD => -1,
        EngineScore::Cp(_) => 0,
    })
}
