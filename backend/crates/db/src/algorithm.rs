//! `algorithm_versions` (ADR 0105): the manifest of every algorithm a row was written with.

use sqlx::PgExecutor;

/// Inserts this build's manifest (`common::algorithm`) once. The worker calls it at start-up;
/// every writer also registers it in its own statement (`common::algorithm::register_cte`), so
/// a row's foreign key never depends on the start-up order.
pub async fn register<'e>(conn: impl PgExecutor<'e>) -> Result<&'static str, sqlx::Error> {
    sqlx::query(
        "INSERT INTO algorithm_versions (id, manifest) VALUES ($1, $2::jsonb) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(common::algorithm::id())
    .bind(common::algorithm::manifest())
    .execute(conn)
    .await?;
    Ok(common::algorithm::id())
}
