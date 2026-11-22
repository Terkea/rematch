//! Persisting enrichment (R2 Task 25): `move_features` rows for one game, replaced as a whole.

use profile_core::types::{MoveFeatures, RawMoveFeatures};
use sqlx::PgConnection;
use uuid::Uuid;

use chess_core::MoveRecord;

use crate::difficulty::{DifficultyOutcome, PendingReason};
use crate::protocol::ENRICHMENT_PROTOCOL;

/// Rows per `INSERT ... SELECT FROM UNNEST(...)`.
pub const PERSIST_CHUNK: usize = 500;

fn json<T: serde::Serialize>(v: &T) -> Result<serde_json::Value, sqlx::Error> {
    serde_json::to_value(v).map_err(|e| sqlx::Error::Encode(e.into()))
}

fn opt_json<T: serde::Serialize>(v: Option<&T>) -> Result<Option<serde_json::Value>, sqlx::Error> {
    v.map(json).transpose()
}

/// Replaces the game's `move_features`: deletes every row, then inserts one per record whose
/// `raw` entry is `Some`, in chunks of [`PERSIST_CHUNK`]. `derived[i]` and `difficulty[i]` (when
/// present) belong to `records[i]`. Run it inside the caller's transaction.
///
/// `difficulty` is [`crate::enrich_game::Enriched::difficulty`]: it sets `difficulty_status` and
/// `difficulty_reason`; an invalid Maia answer counts as the first attempt.
pub async fn persist_move_features(
    conn: &mut PgConnection,
    game_id: Uuid,
    records: &[MoveRecord],
    raw: &[Option<RawMoveFeatures>],
    derived: Option<&[MoveFeatures]>,
    difficulty: &[Option<DifficultyOutcome>],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM move_features WHERE game_id = $1")
        .bind(game_id)
        .execute(&mut *conn)
        .await?;
    let rows: Vec<usize> = (0..records.len())
        .filter(|&i| raw.get(i).is_some_and(Option::is_some))
        .collect();
    for chunk in rows.chunks(PERSIST_CHUNK) {
        insert_chunk(conn, game_id, records, raw, derived, difficulty, chunk).await?;
    }
    Ok(())
}

#[derive(Default)]
struct Columns {
    ply: Vec<i32>,
    multipv: Vec<Option<serde_json::Value>>,
    best_pv: Vec<serde_json::Value>,
    refutation_pv: Vec<serde_json::Value>,
    threat: Vec<Option<serde_json::Value>>,
    tb_before: Vec<Option<i16>>,
    tb_after: Vec<Option<i16>>,
    p_best: Vec<Option<f32>>,
    p_ok: Vec<Option<f32>>,
    model: Vec<Option<String>>,
    is_critical: Vec<Option<bool>>,
    multipv_gap: Vec<Option<f32>>,
    in_top2: Vec<Option<bool>>,
    best_line: Vec<Option<serde_json::Value>>,
    refutation_line: Vec<Option<serde_json::Value>>,
    threat_line: Vec<Option<serde_json::Value>>,
    threat_existed: Vec<Option<bool>>,
    threat_addressed: Vec<Option<bool>>,
    status: Vec<Option<&'static str>>,
    reason: Vec<Option<&'static str>>,
    attempts: Vec<i16>,
}

async fn insert_chunk(
    conn: &mut PgConnection,
    game_id: Uuid,
    records: &[MoveRecord],
    raw: &[Option<RawMoveFeatures>],
    derived: Option<&[MoveFeatures]>,
    difficulty: &[Option<DifficultyOutcome>],
    chunk: &[usize],
) -> Result<(), sqlx::Error> {
    let mut c = Columns::default();
    let none = MoveFeatures::default();
    for &i in chunk {
        let Some(r) = raw.get(i).and_then(Option::as_ref) else {
            continue;
        };
        let f = derived.and_then(|d| d.get(i)).unwrap_or(&none);
        let o = difficulty.get(i).and_then(Option::as_ref);
        c.ply.push(records[i].ply as i32);
        c.multipv.push(opt_json(r.multipv.as_ref())?);
        c.best_pv.push(json(&r.best_pv)?);
        c.refutation_pv.push(json(&r.refutation_pv)?);
        c.threat.push(opt_json(r.threat.as_ref())?);
        c.tb_before.push(r.tb_wdl_before.map(i16::from));
        c.tb_after.push(r.tb_wdl_after.map(i16::from));
        c.p_best
            .push(r.difficulty.as_ref().map(|d| d.p_best as f32));
        c.p_ok.push(r.difficulty.as_ref().map(|d| d.p_ok as f32));
        c.model.push(r.difficulty.as_ref().map(|d| d.model.clone()));
        c.is_critical.push(f.is_critical);
        c.multipv_gap.push(f.multipv_gap.map(|g| g as f32));
        c.in_top2.push(f.in_top2);
        c.best_line.push(opt_json(f.best.as_ref())?);
        c.refutation_line.push(opt_json(f.refutation.as_ref())?);
        c.threat_line.push(opt_json(f.threat.as_ref())?);
        c.threat_existed.push(f.threat_existed);
        c.threat_addressed.push(f.threat_addressed);
        c.status.push(o.map(DifficultyOutcome::status));
        c.reason.push(o.and_then(DifficultyOutcome::reason));
        c.attempts.push(i16::from(matches!(
            o,
            Some(DifficultyOutcome::PendingMaia {
                reason: PendingReason::InvalidResponse
            })
        )));
    }
    sqlx::query(
        "INSERT INTO move_features (game_id, ply, multipv, best_pv, refutation_pv, threat, \
           tb_wdl_before, tb_wdl_after, p_best, p_ok, difficulty_model, is_critical, multipv_gap, \
           in_top2, best_line, refutation_line, threat_line, threat_existed, threat_addressed, \
           enrichment_protocol, difficulty_status, difficulty_reason, difficulty_attempts) \
         SELECT $1, ply, multipv, ARRAY(SELECT jsonb_array_elements_text(best_pv)), \
           ARRAY(SELECT jsonb_array_elements_text(refutation_pv)), threat, tb_before, tb_after, \
           p_best, p_ok, model, is_critical, multipv_gap, in_top2, best_line, refutation_line, \
           threat_line, threat_existed, threat_addressed, $2, status, reason, attempts \
         FROM UNNEST($3::int4[], $4::jsonb[], $5::jsonb[], $6::jsonb[], $7::jsonb[], \
           $8::int2[], $9::int2[], $10::float4[], $11::float4[], $12::text[], $13::bool[], \
           $14::float4[], $15::bool[], $16::jsonb[], $17::jsonb[], $18::jsonb[], $19::bool[], \
           $20::bool[], $21::text[], $22::text[], $23::int2[]) \
           AS t(ply, multipv, best_pv, refutation_pv, threat, tb_before, tb_after, p_best, p_ok, \
             model, is_critical, multipv_gap, in_top2, best_line, refutation_line, threat_line, \
             threat_existed, threat_addressed, status, reason, attempts)",
    )
    .bind(game_id)
    .bind(ENRICHMENT_PROTOCOL)
    .bind(&c.ply)
    .bind(&c.multipv)
    .bind(&c.best_pv)
    .bind(&c.refutation_pv)
    .bind(&c.threat)
    .bind(&c.tb_before)
    .bind(&c.tb_after)
    .bind(&c.p_best)
    .bind(&c.p_ok)
    .bind(&c.model)
    .bind(&c.is_critical)
    .bind(&c.multipv_gap)
    .bind(&c.in_top2)
    .bind(&c.best_line)
    .bind(&c.refutation_line)
    .bind(&c.threat_line)
    .bind(&c.threat_existed)
    .bind(&c.threat_addressed)
    .bind(&c.status)
    .bind(&c.reason)
    .bind(&c.attempts)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
