//! Dedicated connection pools for tests whose timing must not depend on other tests.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// A pool of `n` connections to the test database behind `test_pool`, all opened before it is
/// returned and kept open, so acquiring one never waits.
///
/// The `#[sqlx::test]` pool holds at most 5 connections, and each one is a permit stolen from
/// a master pool of 20 shared by every test in the binary. A child pool keeps the permits it
/// stole until it closes. So an acquire beyond what a test already holds waits for *other tests
/// to finish*, and a runner's lease renewals or a shutdown's writes then miss their deadlines
/// under load. A runner whose deadlines a test asserts gets one of these pools instead. Size it
/// for everything the runner can hold at once: its listener, the main loop's claim or reap,
/// and one renew or write per in-flight job.
///
/// Close it (`pool.close().await`) before the test returns: `#[sqlx::test]` drops the test
/// database afterwards, and a connection still open makes that drop fail.
pub async fn dedicated_pool(test_pool: &PgPool, n: u32) -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(n)
        .min_connections(n)
        .idle_timeout(None)
        .connect_with((*test_pool.connect_options()).clone())
        .await
        .expect("connect a dedicated test pool");
    // Open all `n` now (`min_connections` fills the pool only in the background).
    let mut warm = Vec::new();
    for _ in 0..n {
        warm.push(pool.acquire().await.expect("warm a dedicated test pool"));
    }
    drop(warm);
    pool
}
