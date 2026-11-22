//! Staging the Syzygy tables on a local disk (R2 Task 35, ADR-R2-25). The implementation lives
//! in `lichess-db` next to the pinned manifest and its hashing; see
//! [`lichess_db::syzygy_stage`].

pub use lichess_db::syzygy_stage::{
    STAGE_LOCK, STAGE_MANIFEST, StageReport, stage_tables, stage_tables_with,
};
