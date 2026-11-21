//! R3: the AI coaching layer. After Stockfish and the profile engine, Claude writes a
//! rating-aware coaching narrative and answers the player's questions ("Ask your coach").
//!
//! Task 1 fixes the module tree and the shared types; later R3 tasks fill the modules but do not
//! change those definitions. The Anthropic client is our own reqwest client (ADR-R3-1).

pub mod chat;
pub mod client;
pub mod db;
pub mod entitlements;
pub mod events;
pub mod generate;
pub mod input;
pub mod jobs;
pub mod output;
pub mod prompts;
pub mod segments;
pub mod testing;
pub mod validator;

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
