//! The drill catalogue (port of `output/drills.ts`, improvement-engine §6.4). Drill types are
//! catalogued, not invented by the model: Claude only picks among `drills_for` (ADR-AI-7).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// TS `DRILL_TYPES`, in that order. No doc comments here or on variants: a type doc becomes a
// model-facing description, and variant docs turn the `enum` into a `oneOf` of consts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrillType {
    OwnMistakePuzzles,
    SafeUnsafe,
    ThemedPuzzles,
    LongCalculation,
    MatePatterns,
    ConvertVsEngine,
    DefendVsEngine,
    OpeningTrainer,
    OpeningTrapPuzzles,
    TablebaseDrills,
    ClockPressure,
    CriticalOrNot,
    BlitzSafety,
    ModelGames,
    GuessTheMove,
    SessionRules,
    SlowGames,
    OwnGameAnalysis,
}

impl DrillType {
    /// Every drill type, in TS `DRILL_TYPES` order.
    pub const ALL: [DrillType; 18] = [
        DrillType::OwnMistakePuzzles,
        DrillType::SafeUnsafe,
        DrillType::ThemedPuzzles,
        DrillType::LongCalculation,
        DrillType::MatePatterns,
        DrillType::ConvertVsEngine,
        DrillType::DefendVsEngine,
        DrillType::OpeningTrainer,
        DrillType::OpeningTrapPuzzles,
        DrillType::TablebaseDrills,
        DrillType::ClockPressure,
        DrillType::CriticalOrNot,
        DrillType::BlitzSafety,
        DrillType::ModelGames,
        DrillType::GuessTheMove,
        DrillType::SessionRules,
        DrillType::SlowGames,
        DrillType::OwnGameAnalysis,
    ];

    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            DrillType::OwnMistakePuzzles => "own_mistake_puzzles",
            DrillType::SafeUnsafe => "safe_unsafe",
            DrillType::ThemedPuzzles => "themed_puzzles",
            DrillType::LongCalculation => "long_calculation",
            DrillType::MatePatterns => "mate_patterns",
            DrillType::ConvertVsEngine => "convert_vs_engine",
            DrillType::DefendVsEngine => "defend_vs_engine",
            DrillType::OpeningTrainer => "opening_trainer",
            DrillType::OpeningTrapPuzzles => "opening_trap_puzzles",
            DrillType::TablebaseDrills => "tablebase_drills",
            DrillType::ClockPressure => "clock_pressure",
            DrillType::CriticalOrNot => "critical_or_not",
            DrillType::BlitzSafety => "blitz_safety",
            DrillType::ModelGames => "model_games",
            DrillType::GuessTheMove => "guess_the_move",
            DrillType::SessionRules => "session_rules",
            DrillType::SlowGames => "slow_games",
            DrillType::OwnGameAnalysis => "own_game_analysis",
        }
    }
}

impl std::fmt::Display for DrillType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// TS `BASE`: the allowed drills per weakness id, before `own_game_analysis` is added.
fn base(weakness_id: &str) -> Option<&'static [DrillType]> {
    use DrillType::*;
    Some(match weakness_id {
        "W1" | "W2" | "W11" => &[OwnMistakePuzzles, SafeUnsafe, ThemedPuzzles, SlowGames],
        "W3" => &[ThemedPuzzles],
        "W4" => &[MatePatterns],
        "W5" => &[ConvertVsEngine],
        "W6" => &[DefendVsEngine, ThemedPuzzles],
        "W7" => &[OpeningTrainer, OpeningTrapPuzzles],
        "W8" => &[TablebaseDrills],
        "W9a" | "W9c" | "W9d" => &[ClockPressure, CriticalOrNot, SlowGames],
        "W9b" => &[BlitzSafety, ClockPressure],
        "W10" => &[ModelGames, GuessTheMove],
        "W12" => &[SessionRules],
        _ => return None,
    })
}

/// Allowed drill types for a weakness (TS `drillsFor`). Unknown ids give an empty list; `W3` with
/// variant `deep` is `long_calculation` only; every id but `W12` adds `own_game_analysis`.
/// Doses belong to Milestone 6.
pub fn drills_for(weakness_id: &str, variant: Option<&str>) -> Vec<DrillType> {
    let Some(base) = base(weakness_id) else {
        return Vec::new();
    };
    let mut list = if weakness_id == "W3" && variant == Some("deep") {
        vec![DrillType::LongCalculation]
    } else {
        base.to_vec()
    };
    if weakness_id != "W12" {
        list.push(DrillType::OwnGameAnalysis);
    }
    list
}
