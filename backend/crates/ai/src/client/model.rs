//! Model ids and effort (spec §9, ADR-AI-6). Exact ids, no date suffix.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AiModel {
    #[serde(rename = "claude-sonnet-5-5")]
    Sonnet55,
    #[serde(rename = "claude-haiku-5-5")]
    Haiku55,
}

impl AiModel {
    pub const ALL: [AiModel; 2] = [AiModel::Sonnet55, AiModel::Haiku55];

    pub fn as_str(self) -> &'static str {
        match self {
            AiModel::Sonnet55 => "claude-sonnet-5-5",
            AiModel::Haiku55 => "claude-haiku-5-5",
        }
    }
}

impl fmt::Display for AiModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An id that is not one of [`AiModel::ALL`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown model: {0:?}")]
pub struct UnknownModelId(pub String);

impl FromStr for AiModel {
    type Err = UnknownModelId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AiModel::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| UnknownModelId(s.to_owned()))
    }
}

/// The model per use.
pub struct Models;

impl Models {
    pub const REPORT: AiModel = AiModel::Sonnet55;
    pub const CHAT: AiModel = AiModel::Sonnet55;
    pub const EXPLAIN: AiModel = AiModel::Haiku55;
    pub const WEEKLY: AiModel = AiModel::Haiku55;
}

/// `output_config.effort`. Always sent explicitly (Haiku 5.5 defaults to `medium`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}
