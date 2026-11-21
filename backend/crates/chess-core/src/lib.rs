//! Chess maths, board wrapper and PGN parsing. Free of I/O and of sqlx.

pub mod board;
pub mod fen;
pub mod game_accuracy;
pub mod judgement;
pub mod move_record;
pub mod pgn;
pub mod phase;
pub mod score;
pub mod time_control;
pub mod types;

pub use types::*;

/// This crate's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
