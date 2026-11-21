//! Shared helpers for the engine integration tests.
#![allow(dead_code)]

use std::sync::Arc;

use engine::EngineError;
use engine::pool::{EnginePool, PoolOptions};
use engine::uci::{StartOptions, UciEngine};
use tokio::sync::Semaphore;

/// Process-wide cap on concurrent engine starts. Each test has its own
/// runtime, and a burst of cold Stockfish starts (net loading) can exceed the
/// handshake timeout.
pub static ENGINE_STARTS: Semaphore = Semaphore::const_new(4);

pub async fn start_engine(opts: StartOptions) -> Result<UciEngine, EngineError> {
    let _permit = ENGINE_STARTS.acquire().await.expect("semaphore open");
    UciEngine::start(opts).await
}

pub async fn create_pool(opts: PoolOptions) -> Result<Arc<EnginePool>, EngineError> {
    let _permit = ENGINE_STARTS.acquire().await.expect("semaphore open");
    EnginePool::create(opts).await
}
