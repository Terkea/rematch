//! Reference builds (R2 Task 17): the lifecycle of one opening-explorer or band-reference build.
//!
//! `building` → `active` (after the dump's SHA-256 matched, R-9) → `superseded` (by the next
//! active build of the same kind, whose activation deletes this build's rows). A build whose
//! dump failed verification is `failed` (rows deleted, the previous active build untouched). A
//! `--verify none` development build is `unverified`: its rows stay for inspection, but it is
//! never `active` and nothing reads it.
//!
//! `reference_builds_one_active_uq` (at most one active build per kind) is a plain unique index,
//! so it is checked per statement and cannot be deferred: [`activate_build`] supersedes the old
//! build first and activates the new one in a second statement.
//!
//! `difficulty_model` is stored verbatim (OWNER OVERRIDE 2026-11-21: difficulty comes only from
//! Maia). `None` stays NULL; nothing here substitutes a value.

use std::fmt;
use std::str::FromStr;

use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

/// Errors of every `refdata` storage call.
#[derive(Debug, thiserror::Error)]
pub enum RefdataError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("no such row {0}")]
    NotFound(Uuid),
    #[error("build {id}: cannot go from {from} to {to}")]
    InvalidTransition {
        id: Uuid,
        from: BuildStatus,
        to: BuildStatus,
    },
    #[error("build {id} is {status}, not building: it takes no more rows")]
    NotBuilding { id: Uuid, status: BuildStatus },
    #[error("build {0} has no verified source SHA-256 and cannot be activated")]
    MissingSourceSha(Uuid),
    #[error("not a SHA-256 (64 lowercase hex characters): {0:?}")]
    InvalidSha(String),
    #[error("not a YYYY-MM month: {0:?}")]
    InvalidMonth(String),
    #[error("build {id} ({month}) is not newer than the active build ({active_month})")]
    NotNewer {
        id: Uuid,
        month: String,
        active_month: String,
    },
    #[error("reference game {game} does not belong to build {build}")]
    BuildMismatch { game: Uuid, build: Uuid },
    #[error("value out of range: {0}")]
    OutOfRange(String),
    #[error("decode: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, RefdataError>;

/// Unknown enum label read from the database or parsed from text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} {value:?}")]
pub struct UnknownLabel {
    pub kind: &'static str,
    pub value: String,
}

macro_rules! label_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// The Postgres enum label.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownLabel;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                match s {
                    $($wire => Ok($name::$variant),)+
                    _ => Err(UnknownLabel { kind: $kind, value: s.to_owned() }),
                }
            }
        }
    };
}

label_enum!(
    /// `reference_kind`.
    BuildKind, "reference kind", {
        BandReference => "band_reference",
        OpeningExplorer => "opening_explorer",
    }
);

label_enum!(
    /// `build_status`.
    BuildStatus, "build status", {
        Building => "building",
        Active => "active",
        Superseded => "superseded",
        Failed => "failed",
        Unverified => "unverified",
    }
);

/// A new build, created `building` with an empty `source_sha256` (set by [`set_source_sha`]
/// once the whole dump is hashed).
#[derive(Clone, Debug, PartialEq)]
pub struct NewBuild {
    pub kind: BuildKind,
    /// `YYYY-MM` of the Lichess dump.
    pub source_month: String,
    pub source_url: Option<String>,
    pub params: Value,
    pub protocol: Option<String>,
    pub enrichment_protocol: Option<String>,
    /// Maia model id, or `None` (never a substitute estimate).
    pub difficulty_model: Option<String>,
}

/// One `reference_builds` row.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildRow {
    pub id: Uuid,
    pub kind: BuildKind,
    pub status: BuildStatus,
    pub source_month: String,
    pub source_sha256: String,
    pub source_url: Option<String>,
    pub params: Value,
    pub games_scanned: i64,
    pub protocol: Option<String>,
    pub enrichment_protocol: Option<String>,
    pub difficulty_model: Option<String>,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub activated_at: Option<OffsetDateTime>,
    pub error: Option<String>,
}

#[derive(FromRow)]
struct RawBuild {
    id: Uuid,
    kind: String,
    status: String,
    source_month: String,
    source_sha256: String,
    source_url: Option<String>,
    params: Value,
    games_scanned: i64,
    protocol: Option<String>,
    enrichment_protocol: Option<String>,
    difficulty_model: Option<String>,
    started_at: OffsetDateTime,
    finished_at: Option<OffsetDateTime>,
    activated_at: Option<OffsetDateTime>,
    error: Option<String>,
}

fn parse<T: FromStr<Err = UnknownLabel>>(s: &str) -> Result<T> {
    s.parse()
        .map_err(|e: UnknownLabel| RefdataError::Decode(e.to_string()))
}

impl TryFrom<RawBuild> for BuildRow {
    type Error = RefdataError;

    fn try_from(r: RawBuild) -> Result<Self> {
        Ok(BuildRow {
            id: r.id,
            kind: parse(&r.kind)?,
            status: parse(&r.status)?,
            source_month: r.source_month,
            source_sha256: r.source_sha256,
            source_url: r.source_url,
            params: r.params,
            games_scanned: r.games_scanned,
            protocol: r.protocol,
            enrichment_protocol: r.enrichment_protocol,
            difficulty_model: r.difficulty_model,
            started_at: r.started_at,
            finished_at: r.finished_at,
            activated_at: r.activated_at,
            error: r.error,
        })
    }
}

/// How long an `unverified` development build is kept after it finished
/// ([`delete_stale_unverified`]).
pub const UNVERIFIED_RETENTION: time::Duration = time::Duration::days(30);

/// True for `YYYY-MM` with a month 01-12, so months order as strings.
fn is_month(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit)
        && matches!(
            &s[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

/// True for exactly 64 lowercase hex characters.
fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// Inserts a `building` build and returns its id. `source_month` must be `YYYY-MM`.
pub async fn create_build(pool: &PgPool, b: &NewBuild) -> Result<Uuid> {
    if !is_month(&b.source_month) {
        return Err(RefdataError::InvalidMonth(b.source_month.clone()));
    }
    let id = sqlx::query_scalar(
        "INSERT INTO reference_builds (kind, status, source_month, source_sha256, source_url, \
           params, protocol, enrichment_protocol, difficulty_model) \
         VALUES ($1::reference_kind, 'building', $2, '', $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(b.kind.as_str())
    .bind(&b.source_month)
    .bind(&b.source_url)
    .bind(&b.params)
    .bind(&b.protocol)
    .bind(&b.enrichment_protocol)
    .bind(&b.difficulty_model)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// The build row, or `None`.
pub async fn get_build(pool: &PgPool, id: Uuid) -> Result<Option<BuildRow>> {
    let raw: Option<RawBuild> = sqlx::query_as(
        "SELECT id, kind::text AS kind, status::text AS status, source_month, source_sha256, \
           source_url, params, games_scanned, protocol, enrichment_protocol, difficulty_model, \
           started_at, finished_at, activated_at, error \
         FROM reference_builds WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    raw.map(BuildRow::try_from).transpose()
}

/// Locks a `building` build for an update of its own row; any other status is `NotBuilding`.
async fn lock_building(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    let (_, status, _) = lock_build(conn, id).await?;
    if status != BuildStatus::Building {
        return Err(RefdataError::NotBuilding { id, status });
    }
    Ok(())
}

/// Records the verified SHA-256 of the dump (64 lowercase hex characters) and the number of
/// games scanned. Only a `building` build takes it.
pub async fn set_source_sha(pool: &PgPool, id: Uuid, sha: &str, games_scanned: i64) -> Result<()> {
    if !is_sha256(sha) {
        return Err(RefdataError::InvalidSha(sha.to_owned()));
    }
    if games_scanned < 0 {
        return Err(RefdataError::OutOfRange(format!(
            "games_scanned {games_scanned}"
        )));
    }
    let mut tx = pool.begin().await?;
    lock_building(&mut tx, id).await?;
    sqlx::query("UPDATE reference_builds SET source_sha256 = $2, games_scanned = $3 WHERE id = $1")
        .bind(id)
        .bind(sha)
        .bind(games_scanned)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Records the Maia model that produced the build's difficulty values, or NULL when none did.
/// Only a `building` build takes it.
pub async fn set_difficulty_model(pool: &PgPool, id: Uuid, model: Option<&str>) -> Result<()> {
    let mut tx = pool.begin().await?;
    lock_building(&mut tx, id).await?;
    sqlx::query("UPDATE reference_builds SET difficulty_model = $2 WHERE id = $1")
        .bind(id)
        .bind(model)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Locks the build row and returns its kind, status and SHA-256.
async fn lock_build(conn: &mut PgConnection, id: Uuid) -> Result<(BuildKind, BuildStatus, String)> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT kind::text, status::text, source_sha256 FROM reference_builds \
         WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let (kind, status, sha) = row.ok_or(RefdataError::NotFound(id))?;
    Ok((parse(&kind)?, parse(&status)?, sha))
}

/// Fails with [`RefdataError::NotBuilding`] unless the build is `building`. Takes a share lock
/// on the build row, so a concurrent activation or failure waits for the writer's transaction.
pub(crate) async fn ensure_building(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM reference_builds WHERE id = $1 FOR SHARE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let status: BuildStatus = parse(&status.ok_or(RefdataError::NotFound(id))?)?;
    if status != BuildStatus::Building {
        return Err(RefdataError::NotBuilding { id, status });
    }
    Ok(())
}

/// Deletes every data row of the given builds (explorer, band reference, observations and
/// reference games). The build rows themselves stay.
async fn delete_build_rows(conn: &mut PgConnection, ids: &[Uuid]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    for table in [
        "opening_explorer",
        "band_reference",
        "reference_observations",
        "reference_games",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE build_id = ANY($1)"
        )))
        .bind(ids)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Activates a verified `building` build in one transaction: the previous active build of the
/// same kind becomes `superseded` and loses its rows, then this build becomes `active` (two
/// statements, in that order; see the module doc). Activations of one kind are serialised by a
/// transaction-scoped advisory lock. Activating an already active build is a no-op.
///
/// Refused: a build without a SHA-256 ([`RefdataError::MissingSourceSha`]), any status other
/// than `building`/`active`, and a `source_month` not newer than the active build's
/// ([`RefdataError::NotNewer`]), so an old dump never replaces a newer one.
pub async fn activate_build(pool: &PgPool, id: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    activate_build_in(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

/// [`activate_build`] inside the caller's transaction (pass `&mut tx`), so a finaliser can write
/// its last rows and activate atomically. `conn` must be inside a transaction: the advisory lock
/// and the two-statement swap are transaction-scoped, and this fails (Postgres `SAVEPOINT can
/// only be used in transaction blocks`) on a bare connection. Nothing is committed here.
pub async fn activate_build_in(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    sqlx::query("SAVEPOINT refdata_activate")
        .execute(&mut *conn)
        .await?;
    activate_steps(conn, id).await?;
    sqlx::query("RELEASE SAVEPOINT refdata_activate")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn activate_steps(tx: &mut PgConnection, id: Uuid) -> Result<()> {
    let kind: Option<String> =
        sqlx::query_scalar("SELECT kind::text FROM reference_builds WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let kind: BuildKind = parse(&kind.ok_or(RefdataError::NotFound(id))?)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('refdata:activate:' || $1))")
        .bind(kind.as_str())
        .execute(&mut *tx)
        .await?;
    let (_, status, sha) = lock_build(&mut *tx, id).await?;
    match status {
        BuildStatus::Active => return Ok(()),
        BuildStatus::Building => {}
        from => {
            return Err(RefdataError::InvalidTransition {
                id,
                from,
                to: BuildStatus::Active,
            });
        }
    }
    if sha.is_empty() {
        return Err(RefdataError::MissingSourceSha(id));
    }
    let (month, active_month): (String, Option<String>) = sqlx::query_as(
        "SELECT b.source_month, (SELECT a.source_month FROM reference_builds a \
           WHERE a.kind = b.kind AND a.status = 'active' AND a.id <> b.id) \
         FROM reference_builds b WHERE b.id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(active_month) = active_month
        && month <= active_month
    {
        return Err(RefdataError::NotNewer {
            id,
            month,
            active_month,
        });
    }
    let old: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE reference_builds SET status = 'superseded' \
         WHERE kind = $1::reference_kind AND status = 'active' AND id <> $2 RETURNING id",
    )
    .bind(kind.as_str())
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    delete_build_rows(&mut *tx, &old).await?;
    sqlx::query(
        "UPDATE reference_builds SET status = 'active', activated_at = now(), \
           finished_at = COALESCE(finished_at, now()) WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Deletes `unverified` builds that finished more than `older_than` ago (normally
/// [`UNVERIFIED_RETENTION`]), with their rows (FK cascade); returns how many builds.
pub async fn delete_stale_unverified(pool: &PgPool, older_than: time::Duration) -> Result<u64> {
    let n = sqlx::query(
        "DELETE FROM reference_builds WHERE status = 'unverified' \
         AND finished_at < now() - $1 * interval '1 second'",
    )
    .bind(older_than.as_seconds_f64())
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n)
}

/// Marks a build `failed` with its error and deletes its rows. The active build of the kind is
/// untouched. Allowed from `building`, `unverified` and `failed` (which updates the error).
pub async fn fail_build(pool: &PgPool, id: Uuid, error: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (_, status, _) = lock_build(&mut tx, id).await?;
    if matches!(status, BuildStatus::Active | BuildStatus::Superseded) {
        return Err(RefdataError::InvalidTransition {
            id,
            from: status,
            to: BuildStatus::Failed,
        });
    }
    sqlx::query(
        "UPDATE reference_builds SET status = 'failed', error = $2, finished_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(&mut *tx)
    .await?;
    delete_build_rows(&mut tx, &[id]).await?;
    tx.commit().await?;
    Ok(())
}

/// Marks a `building` build `unverified` (`--verify none`, R-9). Its rows stay, but it is never
/// activated. Marking an unverified build again is a no-op.
pub async fn mark_unverified(pool: &PgPool, id: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    let (_, status, _) = lock_build(&mut tx, id).await?;
    match status {
        BuildStatus::Unverified => return Ok(()),
        BuildStatus::Building => {}
        from => {
            return Err(RefdataError::InvalidTransition {
                id,
                from,
                to: BuildStatus::Unverified,
            });
        }
    }
    sqlx::query(
        "UPDATE reference_builds SET status = 'unverified', finished_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The active build of a kind, if any. `unverified`, `building` and `failed` builds are never
/// returned.
pub async fn active_build(pool: &PgPool, kind: BuildKind) -> Result<Option<Uuid>> {
    let id = sqlx::query_scalar(
        "SELECT id FROM reference_builds WHERE kind = $1::reference_kind AND status = 'active'",
    )
    .bind(kind.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(id)
}
