//! The chat's data source over Postgres (Task 25): ONE account's games, moves, cached evals,
//! stored report sections and coaching input. Every game, move and report query filters on the
//! scope's account, so another account's id is simply not found.
//!
//! - `get_weakness` and `get_opening_line` read the scope's coaching input (the snapshot the
//!   report was written from).
//! - `find_examples` takes the input's exemplars first, then R2's stored occurrences of the
//!   scope (bug hunt I10: with no band reference nothing is ranked, so the input alone has none):
//!   a weakness's per-game `exemplarPlies` (`game_metrics`), a motif's erring moves whose best or
//!   refuting line carries the tag (`move_features`), and an opening line's worst player move in
//!   its first 30 plies, one per game (`game_openings`). Each is looked up in the account's moves.
//! - A position's tags are the matching exemplar's plus its stored line tags (`move_features`),
//!   in the contract's snake_case.
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
    Option<Value>,
    Option<Value>,
);

/// How many plies of an opening line count for its examples (the report's rule, R2 T34).
const LINE_PLIES: i32 = 30;

/// R2's camelCase motif tag (`hangingPiece`, `mateIn4Plus`) in the contract's snake_case
/// (`hanging_piece`, `mate_in_4_plus`): a word per capital, digits as their own word.
fn snake_tag(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    let mut prev_digit = false;
    for c in camel.chars() {
        let digit = c.is_ascii_digit();
        if c.is_ascii_uppercase() || (digit && !prev_digit && !out.is_empty()) {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
        prev_digit = digit;
    }
    out
}

/// The contract's snake_case tag in R2's camelCase (the inverse of [`snake_tag`]).
fn camel_tag(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    for (i, part) in snake.split('_').filter(|p| !p.is_empty()).enumerate() {
        let mut cs = part.chars();
        if i == 0 {
            out.push_str(part);
        } else if let Some(first) = cs.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(cs.as_str());
        }
    }
    out
}

/// The string tags of a stored line's `tags` array, in snake_case.
fn line_tags(v: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(snake_tag)
}

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
             m.time_spent, mf.best_line -> 'tags', mf.refutation_line -> 'tags' \
             FROM moves m JOIN games g ON g.id = m.game_id \
             LEFT JOIN move_features mf ON mf.game_id = m.game_id AND mf.ply = m.ply \
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
            best_tags,
            refutation_tags,
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
        let mut tags = self.tags_of(&gid, ply);
        for t in line_tags(best_tags.as_ref()).chain(line_tags(refutation_tags.as_ref())) {
            if !tags.contains(&t) {
                tags.push(t);
            }
        }
        Ok(Some(PositionRecord {
            tags,
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

    /// R2's stored occurrences of one selector in the scope, newest game first, as `(platform
    /// game id, ply)`; at most `limit` (module docs).
    async fn stored_refs(
        &self,
        q: &ExamplesQuery,
        limit: i64,
    ) -> anyhow::Result<Vec<(String, i32)>> {
        let account = self.scope.account_id;
        let tc = self.scope.time_class.as_str();
        let rows = if let Some(w) = &q.weakness_id {
            // `W3` covers `W3.shallow` and `W3.deep`; `W9b` is its own key.
            sqlx::query_as(
                "SELECT g.platform_game_id, p.ply::int4 FROM game_metrics gm \
                 JOIN games g ON g.id = gm.game_id \
                 CROSS JOIN LATERAL jsonb_each(CASE WHEN jsonb_typeof(gm.metrics -> 'weakness') \
                 = 'object' THEN gm.metrics -> 'weakness' ELSE '{}'::jsonb END) AS w(key, obs) \
                 CROSS JOIN LATERAL jsonb_array_elements_text(CASE \
                 WHEN jsonb_typeof(w.obs -> 'exemplarPlies') = 'array' \
                 THEN w.obs -> 'exemplarPlies' ELSE '[]'::jsonb END) AS p(ply) \
                 WHERE gm.account_id = $1 AND g.account_id = $1 \
                 AND gm.time_class = $2::time_class \
                 AND (w.key = $3 OR left(w.key, length($3) + 1) = $3 || '.') \
                 AND p.ply ~ '^[0-9]{1,4}$' \
                 ORDER BY gm.played_at DESC, g.platform_game_id, p.ply::int4 LIMIT $4",
            )
            .bind(account)
            .bind(tc)
            .bind(w)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else if let Some(m) = &q.motif {
            // The player's own erring moves whose best line (missed) or refutation (allowed)
            // carries the motif.
            sqlx::query_as(
                "SELECT g.platform_game_id, m.ply FROM moves m \
                 JOIN games g ON g.id = m.game_id \
                 JOIN move_features mf ON mf.game_id = m.game_id AND mf.ply = m.ply \
                 WHERE g.account_id = $1 AND g.time_class = $2::time_class \
                 AND m.color = g.player_color AND m.judgement IS NOT NULL \
                 AND (coalesce(mf.best_line -> 'tags', '[]') @> jsonb_build_array($3::text) \
                 OR coalesce(mf.refutation_line -> 'tags', '[]') @> jsonb_build_array($3::text)) \
                 ORDER BY g.played_at DESC, m.delta_w DESC, m.ply LIMIT $4",
            )
            .bind(account)
            .bind(tc)
            .bind(camel_tag(m))
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else if let Some(l) = &q.line_key {
            sqlx::query_as(
                "SELECT platform_game_id, ply FROM ( \
                 SELECT DISTINCT ON (g.id) g.platform_game_id, m.ply, g.played_at \
                 FROM game_openings go JOIN games g ON g.id = go.game_id \
                 JOIN moves m ON m.game_id = g.id \
                 WHERE go.account_id = $1 AND g.account_id = $1 \
                 AND go.time_class = $2::time_class AND go.line_key = $3 \
                 AND m.color = g.player_color AND m.ply <= $5 \
                 ORDER BY g.id, m.delta_w DESC, m.ply) worst \
                 ORDER BY played_at DESC, platform_game_id LIMIT $4",
            )
            .bind(account)
            .bind(tc)
            .bind(l)
            .bind(limit)
            .bind(LINE_PLIES)
            .fetch_all(&self.pool)
            .await?
        } else {
            Vec::new()
        };
        Ok(rows)
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
        if out.len() < limit {
            // A few spare refs: some may repeat an exemplar or no longer have a stored move.
            let want = i64::try_from(limit * 3).unwrap_or(i64::MAX);
            for (game_id, ply) in self.stored_refs(&q, want).await? {
                if out.len() >= limit {
                    break;
                }
                let Ok(ply) = u32::try_from(ply) else {
                    continue;
                };
                if !seen.insert((game_id.clone(), ply)) {
                    continue;
                }
                if let Some(p) = self.position(&game_id, ply).await? {
                    out.push(p);
                }
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
