//! The M3 CoachingInput contract as serde + schemars types, shared with R3 (ADR-R2-2).
//! Never depends on sqlx.

pub mod enums;
pub mod limits;
pub mod schema;
pub mod types;
pub mod validate;
