//! The computed report (Task 14, TS plan B3 `buildComputedReport`): the engine-derived part of a
//! report, shown with or without the AI narrative: player, basis, area scores, metrics,
//! weaknesses, openings and strengths (with their exemplar positions), focuses and changes. Everything here is copied or derived from the
//! `CoachingInput`; nothing comes from the model.

use serde::{Deserialize, Serialize};

use crate::input::types::{
    AreaScore, BandId, Basis, CoachingInput, Metric, OpeningVerdict, Platform, RankedWeakness,
    Strength, TimeClass, TimelineChange, WeaknessState,
};
use crate::validator::focus::select_focuses;
use crate::validator::language::format_evidence_window;

/// Who the report is about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputedPlayer {
    pub platform: Platform,
    pub time_class: TimeClass,
    pub band: BandId,
    /// Null when unknown.
    pub rating: Option<u16>,
}

/// TS `ComputedReport`, camelCase on the wire (stored in `reports.computed`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputedReport {
    pub player: ComputedPlayer,
    pub basis: Basis,
    /// [`format_evidence_window`]: plain text, e.g. "over your last 214 analysed rapid games
    /// (3,912 imported)".
    pub evidence_window: String,
    pub area_scores: Vec<AreaScore>,
    pub metrics: Vec<Metric>,
    pub weaknesses: Vec<RankedWeakness>,
    pub openings: Vec<OpeningVerdict>,
    /// The computed strengths ("What you do well", spec §10.3), each with its exemplar
    /// positions, so the web can show them on a board, also when no AI section is ready.
    pub strengths: Vec<Strength>,
    /// The computed focuses (`select_focuses`); empty when none qualifies (the narrative then
    /// uses the general focus).
    pub focuses: Vec<String>,
    /// Changes whose `to_state` is emerging or active.
    pub new_this_period: Vec<TimelineChange>,
    /// Changes whose `to_state` is resolved.
    pub fixed_this_period: Vec<TimelineChange>,
}

/// The computed report of `input`.
pub fn build_computed_report(input: &CoachingInput, is_first_report: bool) -> ComputedReport {
    let changes = |pred: fn(WeaknessState) -> bool| -> Vec<TimelineChange> {
        input
            .changes
            .iter()
            .filter(|c| pred(c.to_state))
            .cloned()
            .collect()
    };
    ComputedReport {
        player: ComputedPlayer {
            platform: input.platform,
            time_class: input.time_class,
            band: input.band,
            rating: input.rating,
        },
        basis: input.basis.clone(),
        evidence_window: format_evidence_window(&input.basis, input.time_class),
        area_scores: input.area_scores.clone(),
        metrics: input.metrics.clone(),
        weaknesses: input.weaknesses.clone(),
        openings: input.openings.clone(),
        strengths: input.strengths.clone(),
        focuses: select_focuses(input, is_first_report),
        new_this_period: changes(|s| matches!(s, WeaknessState::Emerging | WeaknessState::Active)),
        fixed_this_period: changes(|s| s == WeaknessState::Resolved),
    }
}
