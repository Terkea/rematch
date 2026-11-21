//! Server-resolved rendering of token text (ADR-AI-8). Narrative is stored and streamed as
//! segments; the web renders these and never parses tokens itself.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which engine line of a position a `pv` token reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PvSide {
    /// The best line from the position before the played move.
    Best,
    /// The refutation line from the position after the played move.
    Refutation,
}

impl PvSide {
    pub fn as_str(self) -> &'static str {
        match self {
            PvSide::Best => "best",
            PvSide::Refutation => "refutation",
        }
    }
}

/// One rendered piece of narrative.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Segment {
    Text {
        text: String,
    },
    Move {
        game_id: String,
        ply: u32,
        san: String,
    },
    Best {
        game_id: String,
        ply: u32,
        san: String,
    },
    Pv {
        game_id: String,
        ply: u32,
        side: PvSide,
        n: u32,
        san: String,
    },
    Line {
        line_key: String,
        n: Option<u32>,
        san: String,
    },
    Num {
        path: String,
        value: f64,
        display: String,
    },
    Date {
        path: String,
        date: String,
    },
    Square {
        square: String,
    },
    /// An interactive engine result: `line` None = the tried moves, `n` None = the line's
    /// evaluation text.
    Eng {
        eval_id: String,
        line: Option<u32>,
        n: Option<u32>,
        text: String,
    },
}
