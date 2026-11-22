//! What the coach chat may read (`CoachDataSource`) and the on-demand engine check
//! (`InteractiveEngine`). Both are scoped to ONE account by their implementation (Task 25's
//! `DbCoachDataSource`, Task 24's `DbInteractiveEngine`): an id of another account is simply not
//! found. The tool executor ([`super::execute`]) also checks that every returned record is the
//! one it asked for.

use std::time::Duration;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{Date, OffsetDateTime};

use super::anchor::ChatAnchor;
use crate::entitlements::QuotaWindow;
use crate::input::types::{MoveRef, OpeningVerdict, PlayerColor, RankedWeakness, Score};
use crate::validator::evidence::EngineEvidence;

/// A game's result for the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameResult {
    Win,
    Loss,
    Draw,
}

impl GameResult {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "win" => Some(Self::Win),
            "loss" => Some(Self::Loss),
            "draw" => Some(Self::Draw),
            _ => None,
        }
    }
}

/// One of the player's games (`get_games`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub game_id: String,
    #[serde(with = "common::js_time::iso")]
    pub played_at: OffsetDateTime,
    pub result: GameResult,
    pub color: PlayerColor,
    pub opponent_rating: Option<i32>,
    pub opening_name: Option<String>,
    pub accuracy: Option<f64>,
    pub ply_count: u32,
}

/// One analysed position of the player's games (`get_position`, `find_examples`, anchors).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRecord {
    pub game_id: String,
    pub ply: u32,
    /// The position before `played`.
    pub fen: String,
    pub fen_after_played: String,
    pub played: MoveRef,
    pub best: Option<MoveRef>,
    /// `inaccuracy`, `mistake` or `blunder`.
    pub judgement: Option<String>,
    pub phase: String,
    /// The mover's win percentage, 0..100.
    pub win_before: f64,
    pub win_after: f64,
    /// Seconds on the mover's clock before the move.
    pub clock_before: Option<f64>,
    pub time_spent: Option<f64>,
    /// The engine's best line from `fen`, UCI.
    pub pv: Vec<String>,
    pub tags: Vec<String>,
}

/// A stored analysis of a position (R1's `position_evals`; read-only for the chat).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedEval {
    pub fen: String,
    /// White's point of view.
    pub score: Score,
    pub best_uci: Option<String>,
    pub pv: Vec<String>,
}

/// `get_games`. The executor has validated every field; `limit` is 1..=20.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GamesQuery {
    pub result: Option<GameResult>,
    pub opening_contains: Option<String>,
    pub since: Option<Date>,
    pub limit: u32,
}

/// A move's judgement, mildest first (Postgres' `judgement` enum has the same order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Judgement {
    Inaccuracy,
    Mistake,
    Blunder,
}

impl Judgement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inaccuracy => "inaccuracy",
            Self::Mistake => "mistake",
            Self::Blunder => "blunder",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "inaccuracy" => Some(Self::Inaccuracy),
            "mistake" => Some(Self::Mistake),
            "blunder" => Some(Self::Blunder),
            _ => None,
        }
    }
}

/// A move's game phase (Postgres' `phase` enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Opening,
    Middlegame,
    Endgame,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Middlegame => "middlegame",
            Self::Endgame => "endgame",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "opening" => Some(Self::Opening),
            "middlegame" => Some(Self::Middlegame),
            "endgame" => Some(Self::Endgame),
            _ => None,
        }
    }
}

/// The player's win chance before the move, in plain words: `winning` is at least
/// [`WINNING_FROM`], `losing` at most [`LOSING_TO`], `equal` strictly between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standing {
    Winning,
    Equal,
    Losing,
}

impl Standing {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Winning => "winning",
            Self::Equal => "equal",
            Self::Losing => "losing",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "winning" => Some(Self::Winning),
            "equal" => Some(Self::Equal),
            "losing" => Some(Self::Losing),
            _ => None,
        }
    }
}

/// `winning` starts at this win chance (the mover's, 0..100).
pub const WINNING_FROM: f64 = 70.0;
/// `losing` ends at this win chance.
pub const LOSING_TO: f64 = 30.0;

/// How move-filter results are ordered: newest game first (`Recent`, the default) or the
/// biggest win-chance drop first (`Worst`). Either way a game gives one move, its worst match.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExampleSort {
    #[default]
    Recent,
    Worst,
}

/// `find_examples`' move filter (fix E, N8): the player's OWN moves (`moves.color =
/// games.player_color`) in the scope's account and time class. Every field narrows the search;
/// the executor has validated each one. Without `judgement`, `min_judgement` or `min_delta_w`
/// only erring moves (any judgement) match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MoveFilter {
    pub phase: Option<Phase>,
    /// Exactly this judgement.
    pub judgement: Option<Judgement>,
    /// This judgement or a more severe one.
    pub min_judgement: Option<Judgement>,
    pub standing: Option<Standing>,
    /// Inclusive bounds on the mover's win chance before the move, 0..=100.
    pub min_win_before: Option<u32>,
    pub max_win_before: Option<u32>,
    /// The least win-chance drop of the move, 1..=100.
    pub min_delta_w: Option<u32>,
    /// Only moves made with less than this many seconds on the player's clock.
    pub clock_below: Option<u32>,
    /// The player's colour in the game.
    pub color: Option<PlayerColor>,
    /// Games played on or after / on or before these dates (UTC).
    pub since: Option<Date>,
    pub until: Option<Date>,
    /// Only the player's newest analysed games of the scope, at most this many.
    pub last_games: Option<u32>,
    pub sort: ExampleSort,
}

/// What a move filter matched in the scope (the `searched` part of the tool result).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterCounts {
    /// The scope's time class, e.g. `rapid`.
    pub time_class: String,
    /// Analysed games the filter looked at (after the colour, date and `last_games` limits).
    pub games_searched: u64,
    pub matching_moves: u64,
    pub matching_games: u64,
}

/// `find_examples`. Exactly one selector is `Some`; `limit` is 1..=10.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExamplesQuery {
    pub weakness_id: Option<String>,
    pub motif: Option<String>,
    pub line_key: Option<String>,
    pub filter: Option<MoveFilter>,
    pub limit: u32,
}

/// The player's own data. Every method is scoped to one account by its implementation and
/// returns `None` (or nothing) for anything outside it. There is deliberately no engine method:
/// cached evals are read-only, and interactive checks go through [`InteractiveEngine`].
pub trait CoachDataSource: Send + Sync {
    fn get_weakness<'a>(
        &'a self,
        weakness_id: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<RankedWeakness>>>;

    fn get_games(&self, q: GamesQuery) -> BoxFuture<'_, anyhow::Result<Vec<GameSummary>>>;

    fn get_position<'a>(
        &'a self,
        game_id: &'a str,
        ply: u32,
    ) -> BoxFuture<'a, anyhow::Result<Option<PositionRecord>>>;

    fn get_opening_line<'a>(
        &'a self,
        line_key: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<OpeningVerdict>>>;

    /// The stored eval of `fen`, never computed.
    fn get_cached_eval<'a>(
        &'a self,
        fen: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<CachedEval>>>;

    fn find_examples(&self, q: ExamplesQuery)
    -> BoxFuture<'_, anyhow::Result<Vec<PositionRecord>>>;

    /// What a move filter matches in the scope. `None` when the source cannot count.
    fn count_examples<'a>(
        &'a self,
        _filter: &'a MoveFilter,
    ) -> BoxFuture<'a, anyhow::Result<Option<FilterCounts>>> {
        Box::pin(async { Ok(None) })
    }

    /// The details of an anchor of this account, or `None` for a foreign or unknown one.
    fn resolve_anchor<'a>(
        &'a self,
        anchor: &'a ChatAnchor,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>>;
}

/// The outcome of an interactive engine check.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineCheck {
    Done {
        eval_id: String,
        evidence: EngineEvidence,
    },
    /// Still running after the wait; `wait` or `abandon` it by id.
    Pending { eval_id: String },
    /// The account's engine-check quota is used up (no check was charged).
    Limit {
        window: QuotaWindow,
        resets_at: OffsetDateTime,
    },
    /// Internal detail for logs only; never shown to the model.
    Unavailable { reason: String },
}

/// On-demand engine checks, charged against the account's quota (Task 24). Scoped to one
/// account: `wait` and `abandon` only see that account's eval ids.
pub trait InteractiveEngine: Send + Sync {
    fn request<'a>(
        &'a self,
        base_fen: &'a str,
        moves: &'a [String],
        wait: Duration,
    ) -> BoxFuture<'a, anyhow::Result<EngineCheck>>;

    fn wait<'a>(
        &'a self,
        eval_id: &'a str,
        wait: Duration,
    ) -> BoxFuture<'a, anyhow::Result<EngineCheck>>;

    /// Gives up on a pending check and refunds the quota.
    fn abandon<'a>(&'a self, eval_id: &'a str) -> BoxFuture<'a, anyhow::Result<()>>;
}
