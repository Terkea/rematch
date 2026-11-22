//! The chat's data source over Postgres (Task 25): ONE account's games, moves, cached evals,
//! stored report sections and coaching input. Every game, move and report query filters on the
//! scope's account, so another account's id is simply not found.
//!
//! - `get_weakness`, `get_opening_line` and `find_examples` read the scope's coaching input (the
//!   snapshot the report was written from); `find_examples` then looks each exemplar up in the
//!   account's moves. R2's per-move tags are not stored yet (ruling Q4), so a position's tags are
//!   the tags of the matching exemplar.
//! - `get_position` joins `moves` with the account's `games` by `platform_game_id` (the chat's
//!   game id), with the engine line from `position_evals` (`fen_key`, R1's `ANALYSIS_PROTOCOL`).
//! - `get_cached_eval` reads `position_evals` only (engine data, never computed here).
//! - `get_games` is the scope's time class, newest first, at most [`MAX_GAMES`].
//! - `resolve_anchor` returns records that carry the anchor's own ids (the T22 contract of
//!   `context::anchor_details`).

use std::collections::HashSet;
use std::sync::Arc;

use chess_core::fen::fen_key;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::anchor::ChatAnchor;
use super::data_source::{
    CachedEval, CoachDataSource, ExamplesQuery, GameResult, GameSummary, GamesQuery, PositionRecord,
};
use super::tools::{MAX_EXAMPLES, MAX_GAMES};
use crate::input::parse::parse_coaching_input;
use crate::input::types::{
    CoachingInput, ExemplarPosition, MoveRef, OpeningVerdict, PlayerColor, RankedWeakness, Score,
    TimeClass,
};
use crate::validator::pv::{fen_after, pv_to_san};

/// What one conversation may read: the account, its time class and its coaching input.
#[derive(Clone, Debug)]
pub struct ChatScope {
    pub account_id: Uuid,
    pub time_class: TimeClass,
    pub input: Arc<CoachingInput>,
}

pub struct DbCoachDataSource {
    pool: PgPool,
    scope: ChatScope,
}

/// `\`, `%` and `_` escaped for `LIKE … ESCAPE '\'`, so the model's text is matched literally.
fn like_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

type MoveRow = (
    String,
    i32,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    f64,
    f64,
    Option<f64>,
    Option<f64>,
);

/// `(score_cp, score_mate, best_uci, pv)` of a `position_evals` row.
type EvalRow = (Option<i32>, Option<i32>, Option<String>, Vec<String>);

type GameRow = (
    String,
    OffsetDateTime,
    String,
    String,
    Option<i32>,
    Option<String>,
    Option<f64>,
    i64,
);

const GAME_SQL: &str = "WITH g AS ( \
      SELECT g.id, g.platform_game_id, g.played_at, g.player_color::text AS color, \
        g.opening_name, g.time_class, \
        CASE WHEN g.player_color = 'white' THEN g.black_rating ELSE g.white_rating END AS opp, \
        (CASE WHEN g.player_color = 'white' THEN g.white_accuracy ELSE g.black_accuracy END)::float8 AS acc, \
        coalesce(g.ply_count::bigint, (SELECT count(*) FROM moves m WHERE m.game_id = g.id)) AS plies, \
        CASE WHEN g.result = '1/2-1/2' THEN 'draw' \
          WHEN (g.result = '1-0' AND g.player_color = 'white') \
            OR (g.result = '0-1' AND g.player_color = 'black') THEN 'win' \
          WHEN g.result IN ('1-0', '0-1') THEN 'loss' END AS outcome \
      FROM games g WHERE g.account_id = $1) \
    SELECT platform_game_id, played_at, outcome, color, opp, opening_name, acc, plies FROM g \
    WHERE outcome IS NOT NULL";

fn game_summary(r: GameRow) -> Option<GameSummary> {
    let (game_id, played_at, outcome, color, opp, opening_name, accuracy, plies) = r;
    Some(GameSummary {
        game_id,
        played_at,
        result: GameResult::parse(&outcome)?,
        color: serde_json::from_value::<PlayerColor>(json!(color)).ok()?,
        opponent_rating: opp,
        opening_name,
        accuracy,
        ply_count: u32::try_from(plies).unwrap_or(0),
    })
}

impl DbCoachDataSource {
    pub fn new(pool: PgPool, scope: ChatScope) -> Self {
        DbCoachDataSource { pool, scope }
    }

    pub fn scope(&self) -> &ChatScope {
        &self.scope
    }

    /// Every exemplar position of the input, in input order (weaknesses, strengths, openings,
    /// changes).
    fn exemplars(&self) -> impl Iterator<Item = &ExemplarPosition> {
        let i = &self.scope.input;
        i.weaknesses
            .iter()
            .flat_map(|w| &w.exemplars)
            .chain(i.strengths.iter().flat_map(|s| &s.exemplars))
            .chain(i.openings.iter().flat_map(|o| &o.examples))
            .chain(
                i.changes
                    .iter()
                    .flat_map(|c| c.examples.iter().map(|e| &e.position)),
            )
    }

    fn tags_of(&self, game_id: &str, ply: u32) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in self
            .exemplars()
            .filter(|e| e.game_id == game_id && e.ply == ply)
        {
            for t in &e.tags {
                if !out.contains(t) {
                    out.push(t.clone());
                }
            }
        }
        out
    }

    async fn position(&self, game_id: &str, ply: u32) -> anyhow::Result<Option<PositionRecord>> {
        let Ok(ply_i) = i32::try_from(ply) else {
            return Ok(None);
        };
        let row: Option<MoveRow> = sqlx::query_as(
            "SELECT g.platform_game_id, m.ply, m.fen_before, m.san, m.uci, m.best_uci, \
             m.judgement::text, m.phase::text, m.win_before, m.win_after, m.clock_before, \
             m.time_spent \
             FROM moves m JOIN games g ON g.id = m.game_id \
             WHERE g.account_id = $1 AND g.platform_game_id = $2 AND m.ply = $3 \
             ORDER BY g.played_at DESC LIMIT 1",
        )
        .bind(self.scope.account_id)
        .bind(game_id)
        .bind(ply_i)
        .fetch_optional(&self.pool)
        .await?;
        let Some((
            gid,
            ply_db,
            fen,
            san,
            uci,
            best_uci,
            judgement,
            phase,
            win_before,
            win_after,
            clock_before,
            time_spent,
        )) = row
        else {
            return Ok(None);
        };
        let Some(fen_after_played) = fen_after(&fen, &uci) else {
            return Ok(None);
        };
        let pv: Option<Vec<String>> = sqlx::query_scalar(
            "SELECT pv FROM position_evals WHERE fen_key = $1 AND protocol = $2",
        )
        .bind(fen_key(&fen))
        .bind(engine::ANALYSIS_PROTOCOL)
        .fetch_optional(&self.pool)
        .await?;
        let best = best_uci.and_then(|b| {
            let san = pv_to_san(&fen, std::slice::from_ref(&b))
                .into_iter()
                .next()?;
            Some(MoveRef { san, uci: b })
        });
        let ply = u32::try_from(ply_db).unwrap_or(ply);
        Ok(Some(PositionRecord {
            tags: self.tags_of(&gid, ply),
            game_id: gid,
            ply,
            fen,
            fen_after_played,
            played: MoveRef { san, uci },
            best,
            judgement,
            phase,
            win_before,
            win_after,
            clock_before,
            time_spent,
            pv: pv.unwrap_or_default(),
        }))
    }

    async fn games(&self, q: GamesQuery) -> anyhow::Result<Vec<GameSummary>> {
        let limit = i64::from(q.limit.clamp(1, MAX_GAMES));
        let rows: Vec<GameRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{GAME_SQL} AND time_class = $2::time_class \
             AND ($3::text IS NULL OR outcome = $3) \
             AND ($4::text IS NULL OR opening_name ILIKE '%' || $4 || '%' ESCAPE '\\') \
             AND ($5::date IS NULL OR played_at >= ($5::date)::timestamp AT TIME ZONE 'UTC') \
             ORDER BY played_at DESC, platform_game_id LIMIT $6"
        )))
        .bind(self.scope.account_id)
        .bind(self.scope.time_class.as_str())
        .bind(q.result.map(|r| match r {
            GameResult::Win => "win",
            GameResult::Loss => "loss",
            GameResult::Draw => "draw",
        }))
        .bind(q.opening_contains.as_deref().map(like_literal))
        .bind(q.since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().filter_map(game_summary).collect())
    }

    async fn game(&self, game_id: &str) -> anyhow::Result<Option<GameSummary>> {
        let row: Option<GameRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{GAME_SQL} AND platform_game_id = $2 ORDER BY played_at DESC LIMIT 1"
        )))
        .bind(self.scope.account_id)
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(game_summary))
    }

    async fn cached_eval(&self, fen: &str) -> anyhow::Result<Option<CachedEval>> {
        let row: Option<EvalRow> = sqlx::query_as(
            "SELECT score_cp, score_mate, best_uci, pv FROM position_evals \
             WHERE fen_key = $1 AND protocol = $2",
        )
        .bind(fen_key(fen))
        .bind(engine::ANALYSIS_PROTOCOL)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(cp, mate, best_uci, pv)| {
            let score = match (mate, cp) {
                (Some(m), _) => Score::Mate(m),
                (None, Some(c)) => Score::Cp(c),
                (None, None) => return None,
            };
            Some(CachedEval {
                fen: fen.to_owned(),
                score,
                best_uci,
                pv,
            })
        }))
    }

    async fn examples(&self, q: ExamplesQuery) -> anyhow::Result<Vec<PositionRecord>> {
        let limit = q.limit.clamp(1, MAX_EXAMPLES) as usize;
        let input = &self.scope.input;
        let picked: Vec<&ExemplarPosition> = if let Some(w) = &q.weakness_id {
            input
                .weaknesses
                .iter()
                .filter(|x| &x.id == w)
                .flat_map(|x| &x.exemplars)
                .collect()
        } else if let Some(m) = &q.motif {
            self.exemplars().filter(|e| e.tags.contains(m)).collect()
        } else if let Some(l) = &q.line_key {
            input
                .openings
                .iter()
                .filter(|o| &o.line_key == l)
                .flat_map(|o| &o.examples)
                .collect()
        } else {
            Vec::new()
        };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for e in picked {
            if out.len() >= limit {
                break;
            }
            if !seen.insert((e.game_id.clone(), e.ply)) {
                continue;
            }
            if let Some(p) = self.position(&e.game_id, e.ply).await? {
                out.push(p);
            }
        }
        Ok(out)
    }

    async fn report_section(
        &self,
        report_id: Uuid,
        section: &Value,
    ) -> anyhow::Result<Option<Value>> {
        let Some(key) = section.as_str() else {
            return Ok(None);
        };
        let row: Option<(String, Option<Value>)> = sqlx::query_as(
            "SELECT s.status::text, s.content FROM report_sections s \
             JOIN reports r ON r.id = s.report_id \
             WHERE r.account_id = $1 AND s.report_id = $2 AND s.section = $3",
        )
        .bind(self.scope.account_id)
        .bind(report_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(status, content)| {
            json!({
                "reportId": report_id.to_string(),
                "section": key,
                "status": status,
                "content": content,
            })
        }))
    }

    async fn anchor(&self, anchor: &ChatAnchor) -> anyhow::Result<Option<Value>> {
        Ok(match anchor {
            ChatAnchor::ReportSection { report_id, section } => {
                self.report_section(*report_id, &serde_json::to_value(section)?)
                    .await?
            }
            ChatAnchor::Weakness { weakness_id } => weakness(&self.scope.input, weakness_id)
                .map(serde_json::to_value)
                .transpose()?,
            ChatAnchor::OpeningLine { line_key } => opening(&self.scope.input, line_key)
                .map(serde_json::to_value)
                .transpose()?,
            ChatAnchor::Game { game_id } => self
                .game(game_id)
                .await?
                .map(serde_json::to_value)
                .transpose()?,
            ChatAnchor::Position { game_id, ply } | ChatAnchor::Move { game_id, ply } => self
                .position(game_id, *ply)
                .await?
                .map(serde_json::to_value)
                .transpose()?,
        })
    }
}

fn weakness(input: &CoachingInput, id: &str) -> Option<RankedWeakness> {
    input.weaknesses.iter().find(|w| w.id == id).cloned()
}

fn opening(input: &CoachingInput, line_key: &str) -> Option<OpeningVerdict> {
    input
        .openings
        .iter()
        .find(|o| o.line_key == line_key)
        .cloned()
}

impl CoachDataSource for DbCoachDataSource {
    fn get_weakness<'a>(
        &'a self,
        weakness_id: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<RankedWeakness>>> {
        Box::pin(async move { Ok(weakness(&self.scope.input, weakness_id)) })
    }

    fn get_games(&self, q: GamesQuery) -> BoxFuture<'_, anyhow::Result<Vec<GameSummary>>> {
        Box::pin(self.games(q))
    }

    fn get_position<'a>(
        &'a self,
        game_id: &'a str,
        ply: u32,
    ) -> BoxFuture<'a, anyhow::Result<Option<PositionRecord>>> {
        Box::pin(self.position(game_id, ply))
    }

    fn get_opening_line<'a>(
        &'a self,
        line_key: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<OpeningVerdict>>> {
        Box::pin(async move { Ok(opening(&self.scope.input, line_key)) })
    }

    fn get_cached_eval<'a>(
        &'a self,
        fen: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<CachedEval>>> {
        Box::pin(self.cached_eval(fen))
    }

    fn find_examples(
        &self,
        q: ExamplesQuery,
    ) -> BoxFuture<'_, anyhow::Result<Vec<PositionRecord>>> {
        Box::pin(self.examples(q))
    }

    fn resolve_anchor<'a>(
        &'a self,
        anchor: &'a ChatAnchor,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(self.anchor(anchor))
    }
}

/// The newest coaching input stored for (account, time class): the snapshot of its newest report
/// run. `None` when there is none, or when the stored JSON no longer validates or belongs to
/// another account (logged without content).
pub async fn latest_coaching_input(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> anyhow::Result<Option<CoachingInput>> {
    let raw: Option<Value> = sqlx::query_scalar(
        "SELECT input FROM report_runs WHERE account_id = $1 AND time_class = $2::time_class \
         AND input IS NOT NULL ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(pool)
    .await?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    match parse_coaching_input(&raw) {
        Ok(i) if i.account_id == account_id => Ok(Some(i)),
        Ok(_) => {
            tracing::warn!("stored coaching input belongs to another account; ignored");
            Ok(None)
        }
        Err(issues) => {
            tracing::warn!(
                issues = issues.len(),
                "stored coaching input is invalid; ignored"
            );
            Ok(None)
        }
    }
}
