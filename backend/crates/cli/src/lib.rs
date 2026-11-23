//! The `cli` command-line tool: `analyse`, `db migrate`, and the read-only `profile`, `eta` and
//! `queue-lag` (R2 Task 35), and `recompute-metrics` (ADR-FIX-C-1).

pub mod analyse;
pub mod db_cmd;
pub mod format;
pub mod profile_cmd;
pub mod queue_cmd;
pub mod recompute_cmd;
pub mod report_cmd;

pub use analyse::{AnalyseDeps, CliDeps, Signal, Started, USAGE, run_analyse, run_cli};
pub use format::to_fixed;
