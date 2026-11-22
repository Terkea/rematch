//! The analyse-game hook (R2 Task 31): per-game work that runs inside an analysis attempt, after
//! the main evaluation, without the analysis crate knowing what it computes.
//!
//! The analyse job calls [`AnalyseHook::prepare`] after `build_move_records`, while it still owns
//! the game (heartbeats keep running). The prepared result is written by
//! [`PreparedHook::persist`] inside the moves transaction, under the fence and inside a
//! savepoint, and [`PreparedHook::after_commit`] runs after the `game.analysed` publish.
//!
//! Failure rules (the analysis is never lost to the hook):
//! - [`HookError::Cancelled`] (shutdown, lost lease or deadline) is handled like R1's shutdown
//!   release (or the deadline's retryable `timeout`); nothing is written.
//! - [`HookError::Failed`] is logged and the game is persisted without hook rows, with
//!   `games.enrichment_protocol = NULL`, so `requeue_unenriched` retries it later.
//! - A [`PreparedHook::persist`] error rolls back its savepoint only; the moves and the `done`
//!   transition still commit, with `enrichment_protocol = NULL`.

use chess_core::{MoveRecord, ParsedGame, PositionEval};
use db::games::GameRow;
use futures::future::BoxFuture;
use sqlx::PgConnection;
use tokio_util::sync::CancellationToken;

/// What the hook sees of one analysed game.
#[derive(Clone, Copy)]
pub struct HookInput<'a> {
    /// The game row as read at the start of the attempt.
    pub game: &'a GameRow,
    pub parsed: &'a ParsedGame,
    /// `plies + 1` main evals (White's view); `evals[i]` is the position before ply `i + 1`.
    pub evals: &'a [PositionEval],
    pub records: &'a [MoveRecord],
    /// Fires on shutdown, a lost lease, the deadline, or when a heartbeat finds the game
    /// superseded. The hook must stop promptly and return [`HookError::Cancelled`].
    pub cancel: &'a CancellationToken,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum HookError {
    /// The run's token fired; the attempt is released (or timed out), not failed.
    #[error("analyse hook cancelled")]
    Cancelled,
    /// The hook could not compute its rows; the analysis is persisted without them.
    #[error("analyse hook failed: {0}")]
    Failed(String),
}

/// Computes a game's hook rows (engine work allowed; no writes to the game's analysis).
pub trait AnalyseHook: Send + Sync {
    fn prepare<'a>(
        &'a self,
        input: HookInput<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedHook>, HookError>>;
}

/// A prepared result, written in the analysis transaction.
pub trait PreparedHook: Send {
    /// Runs inside the moves transaction (in a savepoint), after the fenced `done` update and the
    /// moves insert. An error rolls back only the hook's writes.
    fn persist<'a>(&'a self, tx: &'a mut PgConnection) -> BoxFuture<'a, Result<(), sqlx::Error>>;

    /// Runs after the commit and the `game.analysed` publish (best effort; never fails the job).
    /// Not called when `persist` failed or the attempt was superseded.
    fn after_commit(self: Box<Self>) -> BoxFuture<'static, ()>;
}
