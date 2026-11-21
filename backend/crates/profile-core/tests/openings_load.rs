//! R2 Task 16: dataset load time, alone in its own test binary so no other test builds or
//! reads the dataset while it is timed.

use profile_core::openings::dataset::{VENDORED_TSVS, build_openings, openings};
use std::time::{Duration, Instant};

/// One bound for every build: the measured worst case (113 ms, in a test build with
/// `cozy-chess` and `chess-core` at `opt-level = 3`) times 3, the timing rule's margin.
/// A release build takes about 20 ms, inside the brief's 200 ms target. A test build takes
/// 100-113 ms, under 200 ms but without the 3x margin, hence 340 ms (controller ruling).
const LOAD_BOUND: Duration = Duration::from_millis(340);

/// Brief name: `dataset_loads_under_200ms`. The only test in this binary.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "wall-clock bound: release only (cargo test --release -p profile-core --test openings_load)"
)]
fn dataset_loads_under_340ms() {
    let t = Instant::now();
    let fresh = build_openings(&VENDORED_TSVS).expect("vendored tables replay");
    let took = t.elapsed();
    assert!(took < LOAD_BOUND, "took {took:?}, bound {LOAD_BOUND:?}");
    assert!(fresh.len() >= 3000, "{} entries", fresh.len());
    assert_eq!(openings().len(), fresh.len());
}
