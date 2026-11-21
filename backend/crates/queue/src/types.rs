//! Shared queue types (final in Task 1).

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Dead,
    Paused,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Dead => "dead",
            JobState::Paused => "paused",
        }
    }

    /// A live job still holds its dedupe key: queued, running or paused.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            JobState::Queued | JobState::Running | JobState::Paused
        )
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("unknown job state: {0:?}")]
pub struct UnknownJobState(pub String);

impl FromStr for JobState {
    type Err = UnknownJobState;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "queued" => JobState::Queued,
            "running" => JobState::Running,
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            "dead" => JobState::Dead,
            "paused" => JobState::Paused,
            other => return Err(UnknownJobState(other.to_owned())),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub dedupe_key: Option<String>,
    pub user_key: Option<Uuid>,
    pub priority: i16,
    /// Attempts so far, including this one.
    pub attempts: i32,
    pub max_attempts: i32,
    pub lease_token: Uuid,
    pub run_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub started_at: OffsetDateTime,
}

impl Job {
    pub fn is_final_attempt(&self) -> bool {
        self.attempts >= self.max_attempts
    }
}

#[derive(thiserror::Error, Debug)]
pub enum JobError {
    #[error("{message}")]
    Retryable {
        message: String,
        code: Option<String>,
    },
    #[error("{message}")]
    Unrecoverable {
        message: String,
        code: Option<String>,
    },
    /// Re-queue after `delay` without using an attempt (open breaker, shutdown).
    #[error("released: {reason}")]
    Release { delay: Duration, reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnqueueOptions {
    pub dedupe_key: Option<String>,
    pub user_key: Option<Uuid>,
    /// Lower runs first. Default 100.
    pub priority: i16,
    pub run_at: Option<OffsetDateTime>,
    /// Default 3.
    pub max_attempts: i32,
    pub recur_every: Option<Duration>,
}

impl Default for EnqueueOptions {
    fn default() -> Self {
        Self {
            dedupe_key: None,
            user_key: None,
            priority: 100,
            run_at: None,
            max_attempts: 3,
            recur_every: None,
        }
    }
}
