//! Engine-side enrichment: MultiPV, PV completion, threat probe, Syzygy and difficulty
//! (R2 Tasks 23-25). Versioned by its own protocol ids.

pub mod critical;
pub mod difficulty;
pub mod enrich_game;
pub mod persist;
pub mod protocol;
pub mod store;
pub mod tablebase;
pub mod types;
