//! Shared platform types (final in Task 1).

use std::sync::Arc;
use std::time::Duration;

use chess_core::{EngineScore, TimeClass};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::stream::BoxStream;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Lichess,
    Chesscom,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Lichess => "lichess",
            Platform::Chesscom => "chesscom",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub name: String,
    pub rating: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedGame {
    pub platform: Platform,
    pub platform_game_id: String,
    pub pgn: String,
    pub played_at: DateTime<Utc>,
    pub time_class: TimeClass,
    pub base_seconds: Option<i32>,
    pub increment_seconds: Option<i32>,
    pub rated: bool,
    pub white: Player,
    pub black: Player,
    /// "1-0" | "0-1" | "1/2-1/2" | "*"
    pub result: String,
    pub termination: Option<String>,
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    /// lichess_evals[i] is the white-POV eval after ply i+1.
    pub lichess_evals: Option<Vec<Option<EngineScore>>>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PlatformError {
    #[error("Not found: {url}")]
    NotFound { url: String },
    #[error("Rate limited (HTTP 429) after {attempts} attempts: {url}")]
    RateLimited { url: String, attempts: u32 },
    #[error("No response within {ms} ms: {url}")]
    Timeout { url: String, ms: u64 },
    #[error("{platform} request failed with HTTP {status}: {url}")]
    Http {
        platform: String,
        status: u16,
        url: String,
    },
    #[error("Circuit open: platform calls paused for another {} ms", retry_in.as_millis())]
    CircuitOpen { retry_in: Duration },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("invalid response: {0}")]
    Decode(String),
}

pub type Headers = Vec<(String, String)>;

pub trait PlatformClient: Send + Sync {
    fn get_json<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxFuture<'a, Result<serde_json::Value, PlatformError>>;
    fn get_ndjson<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxStream<'a, Result<serde_json::Value, PlatformError>>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchOptions {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub max: Option<u32>,
}

pub trait GameSource: Send + Sync {
    fn fetch<'a>(
        &'a self,
        client: Arc<dyn PlatformClient>,
        username: &'a str,
        opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>>;
}
