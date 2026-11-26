//! The M3 CoachingInput contract as serde + schemars types, shared with R3 (ADR-R2-2).
//! Never depends on sqlx.

// `#[macro_use]` makes Task 1's `wire_enum!` available to `types.rs` (declared below).
#[macro_use]
pub mod enums;
pub mod limits;
pub mod motifs;
pub mod schema;
pub mod trajectory;
pub mod types;
pub mod validate;

pub use enums::*;
pub use limits::{
    INPUT_VERSION, INPUT_VERSION_V1, INPUT_VERSION_V2, LIMITS, Limits, MIN_COLOR_GAMES,
};
pub use schema::json_schema;
pub use types::*;
pub use validate::Issue;
