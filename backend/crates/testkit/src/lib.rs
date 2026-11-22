//! Shared test helpers (dev-dependency of every library crate).

pub mod env;
pub mod fake_engine;
pub mod fake_platform;
pub mod fixtures;
pub mod pool;
pub mod seed;
pub mod wait;

pub use env::{in_docker, stockfish};
pub use fixtures::fixture;
pub use pool::dedicated_pool;
pub use seed::{GameSeed, SCHOLARS, insert_account, insert_game, insert_user, insert_user_with_id};
pub use wait::wait_for;
