//! The `CoachingInput` contract: R2's `coaching-input` crate, re-exported (fix round 1, Option A).
//!
//! There is one contract type. R2's crate mirrors the M3 zod schema field for field, with
//! strict objects, present-but-nullable keys and typed dates. `coaching_input::json_schema()`
//! applies `contract_transform` (every property required, `Option` stays `T | null`). That is
//! the one mechanism for required + nullable schemas, and Task 3's output schemas reuse it.
//! `tests/input_shape.rs` checks the type against the zod export in
//! `fixtures/ai/ts-export/coaching-input.schema.json`.

pub use coaching_input::schema::contract_transform;
pub use coaching_input::*;
