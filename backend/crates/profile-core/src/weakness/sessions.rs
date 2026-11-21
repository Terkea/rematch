//! Session grouping and expected scores (R2 Task 7, rules TS M2 Task 6; improvement-engine
//! section 6.2 W12).
//!
//! Games are sorted by `played_at` (ties by `game_id`). A new session starts when
//! `played_at - (prev.played_at + prev.est_duration) >= gap`. `after_loss` means the previous game
//! of the same session was lost; `residual = score - expected_score` when both are known.

use std::collections::HashSet;

use time::OffsetDateTime;
use uuid::Uuid;

use crate::types::SessionInfo;

/// Default gap between sessions, in minutes.
pub const SESSION_GAP_MINUTES: i64 = 15;

/// Moves assumed per game when estimating its duration from the increment.
pub const EST_MOVES_PER_GAME: f64 = 40.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionGame {
    pub game_id: Uuid,
    pub played_at: OffsetDateTime,
    pub est_duration_seconds: f64,
    /// Player's point of view: 1, 0.5 or 0.
    pub score: Option<f64>,
    pub expected_score: Option<f64>,
}

/// [`build_sessions`] with its counts of invalid inputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionBuild {
    pub sessions: Vec<SessionInfo>,
    /// Games whose `score` (not 0, 0.5 or 1) or `expected_score` (not finite in [0, 1]) was
    /// invalid; that value is treated as unknown.
    pub invalid_scores: u32,
    /// Games whose `est_duration_seconds` is not a finite value `>= 0`. They stay in the timeline
    /// with duration 0.
    pub invalid_durations: u32,
    /// Repeated `game_id`s dropped: only the earliest copy (by `played_at`, then input order) is
    /// kept.
    pub duplicate_games: u32,
}

/// `base + 40 * increment` seconds; `None` without a base (daily games).
pub fn est_duration_seconds(base: Option<u32>, increment: Option<u32>) -> Option<f64> {
    let base = f64::from(base?);
    Some(base + EST_MOVES_PER_GAME * f64::from(increment.unwrap_or(0)))
}

/// Elo expected score `1 / (1 + 10^((opp - own) / 400))`. Finite for finite inputs; prefer
/// [`expected_score_from_ratings`], which cannot produce NaN.
pub fn expected_score(own: f64, opp: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((opp - own) / 400.0))
}

/// [`expected_score`] when both ratings are known.
pub fn expected_score_from_ratings(own: Option<i32>, opp: Option<i32>) -> Option<f64> {
    Some(expected_score(f64::from(own?), f64::from(opp?)))
}

pub fn build_sessions(games: &[SessionGame], gap_minutes: i64) -> Vec<SessionInfo> {
    build_sessions_counted(games, gap_minutes).sessions
}

pub fn build_sessions_counted(games: &[SessionGame], gap_minutes: i64) -> SessionBuild {
    let mut sorted: Vec<&SessionGame> = games.iter().collect();
    // Stable: equal (played_at, game_id) keep their input order.
    sorted.sort_by_key(|g| (g.played_at, g.game_id));
    let mut seen = HashSet::new();
    let before = sorted.len();
    sorted.retain(|g| seen.insert(g.game_id));
    let duplicate_games = (before - sorted.len()) as u32;

    let gap_seconds = gap_minutes as f64 * 60.0;
    let mut invalid_scores = 0;
    let mut invalid_durations = 0;
    let mut sessions = Vec::with_capacity(sorted.len());
    let mut session_id = 0;
    let mut index = 0;
    // (played_at, duration, valid score) of the previous game.
    let mut prev: Option<(OffsetDateTime, f64, Option<f64>)> = None;
    for g in sorted {
        let duration = if g.est_duration_seconds.is_finite() && g.est_duration_seconds >= 0.0 {
            g.est_duration_seconds
        } else {
            invalid_durations += 1;
            0.0
        };
        let score = g
            .score
            .filter(|s| (*s == 0.0) || (*s == 0.5) || (*s == 1.0));
        let expected = g
            .expected_score
            .filter(|e| e.is_finite() && (0.0..=1.0).contains(e));
        if score.is_none() != g.score.is_none() || expected.is_none() != g.expected_score.is_none()
        {
            invalid_scores += 1;
        }
        let new_session = match prev {
            None => true,
            Some((at, dur, _)) => (g.played_at - at).as_seconds_f64() - dur >= gap_seconds,
        };
        let after_loss = if new_session {
            session_id += 1;
            index = 1;
            false
        } else {
            index += 1;
            prev.and_then(|(_, _, s)| s) == Some(0.0)
        };
        let residual = match (score, expected) {
            (Some(s), Some(e)) => Some(s - e),
            _ => None,
        };
        sessions.push(SessionInfo {
            game_id: g.game_id,
            session_id,
            index_in_session: index,
            after_loss,
            residual,
        });
        prev = Some((g.played_at, duration, score));
    }
    SessionBuild {
        sessions,
        invalid_scores,
        invalid_durations,
        duplicate_games,
    }
}
