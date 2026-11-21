//! R2 Task 17: reference build lifecycle (`refdata::builds`). TS M2 Task 11 build cases at the
//! storage level, plus the R2 rules: verification before activation (R-9), `unverified` builds
//! never active, supersede-then-activate under the non-deferrable `reference_builds_one_active_uq`.

use refdata::builds::{
    BuildKind, BuildStatus, NewBuild, RefdataError, activate_build, active_build, create_build,
    fail_build, get_build, mark_unverified, set_difficulty_model, set_source_sha,
};
use refdata::explorer::{ExplorerCounts, ExplorerKey, flush_explorer};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use chess_core::TimeClass;
use profile_core::types::Band;

fn new_build(kind: BuildKind, month: &str) -> NewBuild {
    NewBuild {
        kind,
        source_month: month.into(),
        source_url: Some(format!(
            "https://database.lichess.org/standard/lichess_db_standard_rated_{month}.pgn.zst"
        )),
        params: json!({ "max_games": 2_000_000, "max_ply": 30 }),
        protocol: None,
        enrichment_protocol: None,
        difficulty_model: None,
    }
}

/// A verified explorer build with one row.
async fn verified_explorer(pool: &PgPool, month: &str) -> Uuid {
    let id = create_build(pool, &new_build(BuildKind::OpeningExplorer, month))
        .await
        .unwrap();
    flush_explorer(pool, id, &[one_row()]).await.unwrap();
    set_source_sha(pool, id, &"ab".repeat(32), 3).await.unwrap();
    id
}

fn one_row() -> (ExplorerKey, ExplorerCounts) {
    (
        ExplorerKey {
            fen_key: "start".into(),
            band: Band::B2,
            time_class: TimeClass::Blitz,
            uci: "e2e4".into(),
        },
        ExplorerCounts {
            white: 1,
            draws: 0,
            black: 0,
        },
    )
}

async fn explorer_rows(pool: &PgPool, build: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM opening_explorer WHERE build_id = $1")
        .bind(build)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn create_build_starts_building_and_round_trips(pool: PgPool) {
    let mut nb = new_build(BuildKind::BandReference, "2026-10");
    nb.protocol = Some("sf19-n300k-v1".into());
    nb.enrichment_protocol = Some("enr-v1".into());
    let id = create_build(&pool, &nb).await.unwrap();
    let b = get_build(&pool, id).await.unwrap().unwrap();
    assert_eq!(b.id, id);
    assert_eq!(b.kind, BuildKind::BandReference);
    assert_eq!(b.status, BuildStatus::Building);
    assert_eq!(b.source_month, "2026-10");
    assert_eq!(b.source_sha256, "");
    assert_eq!(b.source_url, nb.source_url);
    assert_eq!(b.params, nb.params);
    assert_eq!(b.games_scanned, 0);
    assert_eq!(b.protocol.as_deref(), Some("sf19-n300k-v1"));
    assert_eq!(b.enrichment_protocol.as_deref(), Some("enr-v1"));
    assert_eq!(b.activated_at, None);
    assert_eq!(b.finished_at, None);
    assert_eq!(b.error, None);
    assert!(get_build(&pool, Uuid::new_v4()).await.unwrap().is_none());

    set_source_sha(&pool, id, "cd", 1234).await.unwrap();
    let b = get_build(&pool, id).await.unwrap().unwrap();
    assert_eq!(b.source_sha256, "cd");
    assert_eq!(b.games_scanned, 1234);
    assert!(matches!(
        set_source_sha(&pool, Uuid::new_v4(), "cd", 1).await,
        Err(RefdataError::NotFound(_))
    ));
}

/// OWNER OVERRIDE (difficulty is Maia-only): a missing model stays NULL. Nothing substitutes a
/// value, and clearing it writes NULL again.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn difficulty_model_is_stored_verbatim_and_none_stays_null(pool: PgPool) {
    let id = create_build(&pool, &new_build(BuildKind::BandReference, "2026-10"))
        .await
        .unwrap();
    let raw: Option<String> =
        sqlx::query_scalar("SELECT difficulty_model FROM reference_builds WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(raw, None);
    assert_eq!(
        get_build(&pool, id)
            .await
            .unwrap()
            .unwrap()
            .difficulty_model,
        None
    );

    set_difficulty_model(&pool, id, Some("maia2-rapid"))
        .await
        .unwrap();
    assert_eq!(
        get_build(&pool, id)
            .await
            .unwrap()
            .unwrap()
            .difficulty_model
            .as_deref(),
        Some("maia2-rapid")
    );
    set_difficulty_model(&pool, id, None).await.unwrap();
    let raw: Option<String> =
        sqlx::query_scalar("SELECT difficulty_model FROM reference_builds WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(raw, None);

    let mut nb = new_build(BuildKind::BandReference, "2026-11");
    nb.difficulty_model = Some("maia2-blitz".into());
    let id2 = create_build(&pool, &nb).await.unwrap();
    assert_eq!(
        get_build(&pool, id2)
            .await
            .unwrap()
            .unwrap()
            .difficulty_model
            .as_deref(),
        Some("maia2-blitz")
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn activate_build_supersedes_and_deletes_the_old_rows(pool: PgPool) {
    let first = verified_explorer(&pool, "2026-09").await;
    activate_build(&pool, first).await.unwrap();
    let b = get_build(&pool, first).await.unwrap().unwrap();
    assert_eq!(b.status, BuildStatus::Active);
    assert!(b.activated_at.is_some());
    assert!(b.finished_at.is_some());
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(first)
    );

    let second = verified_explorer(&pool, "2026-10").await;
    // Activating supersedes `first` before activating `second`: the partial unique index
    // `reference_builds_one_active_uq` is not deferrable, so the reverse order would fail.
    activate_build(&pool, second).await.unwrap();
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(second)
    );
    assert_eq!(
        get_build(&pool, first).await.unwrap().unwrap().status,
        BuildStatus::Superseded
    );
    assert_eq!(explorer_rows(&pool, first).await, 0);
    assert_eq!(explorer_rows(&pool, second).await, 1);
    // The other kind is untouched.
    assert_eq!(
        active_build(&pool, BuildKind::BandReference).await.unwrap(),
        None
    );
}

/// Why `activate_build` uses two statements in that order: with the old build still active,
/// marking the new one active first is a unique violation (the index is checked per statement
/// and cannot be deferred).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_active_index_rejects_activating_before_superseding(pool: PgPool) {
    let first = verified_explorer(&pool, "2026-09").await;
    activate_build(&pool, first).await.unwrap();
    let second = verified_explorer(&pool, "2026-10").await;

    // A plain unique index: enforced immediately, never deferrable.
    let (unique, immediate): (bool, bool) = sqlx::query_as(
        "SELECT i.indisunique, i.indimmediate FROM pg_index i \
         JOIN pg_class c ON c.oid = i.indexrelid WHERE c.relname = 'reference_builds_one_active_uq'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(unique && immediate);

    let mut tx = pool.begin().await.unwrap();
    let err = sqlx::query("UPDATE reference_builds SET status = 'active' WHERE id = $1")
        .bind(second)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23505"),
        "{err}"
    );
    tx.rollback().await.unwrap();

    activate_build(&pool, second).await.unwrap();
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(second)
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn activate_requires_a_verified_building_build(pool: PgPool) {
    let id = create_build(&pool, &new_build(BuildKind::OpeningExplorer, "2026-10"))
        .await
        .unwrap();
    // No SHA-256 recorded yet: never activated (R-9).
    assert!(matches!(
        activate_build(&pool, id).await,
        Err(RefdataError::MissingSourceSha(x)) if x == id
    ));
    assert_eq!(
        get_build(&pool, id).await.unwrap().unwrap().status,
        BuildStatus::Building
    );
    assert!(matches!(
        activate_build(&pool, Uuid::new_v4()).await,
        Err(RefdataError::NotFound(_))
    ));

    set_source_sha(&pool, id, "ab", 1).await.unwrap();
    activate_build(&pool, id).await.unwrap();
    let at = get_build(&pool, id).await.unwrap().unwrap().activated_at;
    // A second activation (two finalisers) is a no-op.
    activate_build(&pool, id).await.unwrap();
    assert_eq!(
        get_build(&pool, id).await.unwrap().unwrap().activated_at,
        at
    );

    // A superseded build cannot come back.
    let next = verified_explorer(&pool, "2026-11").await;
    activate_build(&pool, next).await.unwrap();
    assert!(matches!(
        activate_build(&pool, id).await,
        Err(RefdataError::InvalidTransition {
            from: BuildStatus::Superseded,
            to: BuildStatus::Active,
            ..
        })
    ));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn failed_build_keeps_previous_active(pool: PgPool) {
    let first = verified_explorer(&pool, "2026-09").await;
    activate_build(&pool, first).await.unwrap();
    let second = verified_explorer(&pool, "2026-10").await;

    fail_build(&pool, second, "sha256 mismatch").await.unwrap();
    let b = get_build(&pool, second).await.unwrap().unwrap();
    assert_eq!(b.status, BuildStatus::Failed);
    assert_eq!(b.error.as_deref(), Some("sha256 mismatch"));
    assert!(b.finished_at.is_some());
    assert_eq!(b.activated_at, None);
    // The failed build's partial rows are gone; the active build is untouched.
    assert_eq!(explorer_rows(&pool, second).await, 0);
    assert_eq!(explorer_rows(&pool, first).await, 1);
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(first)
    );
    // A failed build never activates and takes no more rows.
    assert!(matches!(
        activate_build(&pool, second).await,
        Err(RefdataError::InvalidTransition {
            from: BuildStatus::Failed,
            ..
        })
    ));
    assert!(matches!(
        flush_explorer(&pool, second, &[one_row()]).await,
        Err(RefdataError::NotBuilding {
            status: BuildStatus::Failed,
            ..
        })
    ));
    assert_eq!(explorer_rows(&pool, second).await, 0);
    // The active build cannot be failed (that would leave the kind without data).
    assert!(matches!(
        fail_build(&pool, first, "x").await,
        Err(RefdataError::InvalidTransition {
            from: BuildStatus::Active,
            to: BuildStatus::Failed,
            ..
        })
    ));
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(first)
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unverified_build_is_never_returned_as_active(pool: PgPool) {
    let dev = verified_explorer(&pool, "2026-10").await;
    mark_unverified(&pool, dev).await.unwrap();
    let b = get_build(&pool, dev).await.unwrap().unwrap();
    assert_eq!(b.status, BuildStatus::Unverified);
    assert!(b.finished_at.is_some());
    // Rows are kept for development, but nothing reads them as the active build.
    assert_eq!(explorer_rows(&pool, dev).await, 1);
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        None
    );
    assert!(matches!(
        activate_build(&pool, dev).await,
        Err(RefdataError::InvalidTransition {
            from: BuildStatus::Unverified,
            to: BuildStatus::Active,
            ..
        })
    ));
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        None
    );

    // With a verified build active, an unverified one still does not replace it.
    let real = verified_explorer(&pool, "2026-09").await;
    activate_build(&pool, real).await.unwrap();
    let dev2 = verified_explorer(&pool, "2026-11").await;
    mark_unverified(&pool, dev2).await.unwrap();
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(real)
    );
    // Only a building build can be marked unverified.
    assert!(matches!(
        mark_unverified(&pool, real).await,
        Err(RefdataError::InvalidTransition {
            from: BuildStatus::Active,
            to: BuildStatus::Unverified,
            ..
        })
    ));
}

/// Two finalisers activating different builds of one kind at once: both succeed in turn, one
/// build ends active, and no unique violation escapes.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_activations_leave_exactly_one_active(pool: PgPool) {
    let old = verified_explorer(&pool, "2026-08").await;
    activate_build(&pool, old).await.unwrap();
    let a = verified_explorer(&pool, "2026-09").await;
    let b = verified_explorer(&pool, "2026-10").await;
    let (ra, rb) = tokio::join!(activate_build(&pool, a), activate_build(&pool, b));
    ra.unwrap();
    rb.unwrap();
    let statuses: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status::text FROM reference_builds WHERE kind = 'opening_explorer' \
         AND status = 'active'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(statuses.len(), 1, "{statuses:?}");
    let winner = statuses[0].0;
    assert!(winner == a || winner == b);
    let loser = if winner == a { b } else { a };
    assert_eq!(
        get_build(&pool, loser).await.unwrap().unwrap().status,
        BuildStatus::Superseded
    );
    assert_eq!(explorer_rows(&pool, loser).await, 0);
    assert_eq!(explorer_rows(&pool, old).await, 0);
    assert_eq!(explorer_rows(&pool, winner).await, 1);
}

#[test]
fn kind_and_status_strings_match_the_enums() {
    for (k, s) in [
        (BuildKind::BandReference, "band_reference"),
        (BuildKind::OpeningExplorer, "opening_explorer"),
    ] {
        assert_eq!(k.as_str(), s);
        assert_eq!(s.parse::<BuildKind>().unwrap(), k);
    }
    for (st, s) in [
        (BuildStatus::Building, "building"),
        (BuildStatus::Active, "active"),
        (BuildStatus::Superseded, "superseded"),
        (BuildStatus::Failed, "failed"),
        (BuildStatus::Unverified, "unverified"),
    ] {
        assert_eq!(st.as_str(), s);
        assert_eq!(s.parse::<BuildStatus>().unwrap(), st);
    }
    assert!("live".parse::<BuildStatus>().is_err());
}
