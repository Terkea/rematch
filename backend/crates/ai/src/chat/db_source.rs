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
//! - A move filter (fix E, N8) reads the player's own moves of the scope directly
//!   ([`FILTER_SQL`]): one per game, its worst match, newest game first (or worst first), with
//!   counts of what it searched and matched (`count_examples`).
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
    CachedEval, CoachDataSource, ExampleSort, ExamplesQuery, FilterCounts, GameResult, GameSummary,
    GamesQuery, Judgement, LOSING_TO, MoveFilter, Phase, PositionRecord, Standing, WINNING_FROM,
};
use super::tools::{MAX_EXAMPLES, MAX_GAMES};
use crate::input::parse::parse_coaching_input;
use crate::input::types::{
    CoachingInput, ExemplarPosition, InputVersion, MoveRef, OpeningVerdict, PlayerColor,
    RankedWeakness, Score, TimeClass,
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

/// The move filter's matches (fix E, N8): `sg` is the scope's analysed games (account, time
/// class, colour, dates, newest `last_games`), `hit` the player's own moves in them that pass
/// every filter. Binds: $1 account, $2 time class, $3 colour, $4 since, $5 until, $6 last games,
/// $7 phase, $8 judgement, $9 min judgement, $10 standing, $11 min win, $12 max win, $13 min
/// drop, $14 clock below; callers add their own after these.
const FILTER_SQL: &str = "WITH sg AS ( \
      SELECT g.id, g.platform_game_id, g.played_at, g.player_color FROM games g \
      WHERE g.account_id = $1 AND g.time_class = $2::time_class \
      AND ($3::text IS NULL OR g.player_color = $3::color) \
      AND ($4::date IS NULL OR g.played_at >= ($4::date)::timestamp AT TIME ZONE 'UTC') \
      AND ($5::date IS NULL OR g.played_at < ($5::date + 1)::timestamp AT TIME ZONE 'UTC') \
      AND EXISTS (SELECT 1 FROM moves am WHERE am.game_id = g.id) \
      ORDER BY g.played_at DESC, g.platform_game_id LIMIT $6), \
    hit AS ( \
      SELECT sg.id AS gid, sg.platform_game_id, sg.played_at, m.ply, m.delta_w FROM sg \
      JOIN moves m ON m.game_id = sg.id \
      WHERE m.color = sg.player_color \
      AND ($7::text IS NULL OR m.phase = $7::phase) \
      AND ($8::text IS NULL OR m.judgement = $8::judgement) \
      AND ($9::text IS NULL OR m.judgement >= $9::judgement) \
      AND ($8::text IS NOT NULL OR $9::text IS NOT NULL OR $13::float8 IS NOT NULL \
        OR m.judgement IS NOT NULL) \
      AND ($10::text IS NULL \
        OR ($10 = 'winning' AND m.win_before >= $15) \
        OR ($10 = 'losing' AND m.win_before <= $16) \
        OR ($10 = 'equal' AND m.win_before > $16 AND m.win_before < $15)) \
      AND ($11::float8 IS NULL OR m.win_before >= $11) \
      AND ($12::float8 IS NULL OR m.win_before <= $12) \
      AND ($13::float8 IS NULL OR m.delta_w >= $13) \
      AND ($14::float8 IS NULL OR m.clock_before < $14))";

/// Binds [`FILTER_SQL`]'s parameters ($1..$16) to a query.
macro_rules! bind_filter {
    ($q:expr, $scope:expr, $f:expr) => {{
        let f: &MoveFilter = $f;
        let num = |n: Option<u32>| n.map(f64::from);
        $q.bind($scope.account_id)
            .bind($scope.time_class.as_str())
            .bind(f.color.map(PlayerColor::as_str))
            .bind(f.since)
            .bind(f.until)
            .bind(f.last_games.map(i64::from))
            .bind(f.phase.map(Phase::as_str))
            .bind(f.judgement.map(Judgement::as_str))
            .bind(f.min_judgement.map(Judgement::as_str))
            .bind(f.standing.map(Standing::as_str))
            .bind(num(f.min_win_before))
            .bind(num(f.max_win_before))
            .bind(num(f.min_delta_w))
            .bind(num(f.clock_below))
            .bind(WINNING_FROM)
            .bind(LOSING_TO)
    }};
}

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

    /// The move filter's examples as `(platform game id, ply)`: each matching game's worst
    /// match (biggest drop, then lowest ply), newest game first or worst first; at most `limit`.
    async fn filter_refs(&self, f: &MoveFilter, limit: i64) -> anyhow::Result<Vec<(String, i32)>> {
        let order = match f.sort {
            ExampleSort::Recent => "played_at DESC, platform_game_id",
            ExampleSort::Worst => "delta_w DESC, played_at DESC, platform_game_id",
        };
        let sql = format!(
            "{FILTER_SQL} SELECT platform_game_id, ply FROM ( \
             SELECT DISTINCT ON (gid) gid, platform_game_id, played_at, ply, delta_w FROM hit \
             ORDER BY gid, delta_w DESC, ply) worst ORDER BY {order} LIMIT $17"
        );
        let q = sqlx::query_as::<_, (String, i32)>(sqlx::AssertSqlSafe(sql));
        Ok(bind_filter!(q, self.scope, f)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?)
    }

    async fn filter_counts(&self, f: &MoveFilter) -> anyhow::Result<FilterCounts> {
        let sql = format!(
            "{FILTER_SQL} SELECT (SELECT count(*) FROM sg), (SELECT count(*) FROM hit), \
             (SELECT count(DISTINCT gid) FROM hit)"
        );
        let q = sqlx::query_as::<_, (i64, i64, i64)>(sqlx::AssertSqlSafe(sql));
        let (games, moves, hit_games) =
            bind_filter!(q, self.scope, f).fetch_one(&self.pool).await?;
        let n = |x: i64| u64::try_from(x).unwrap_or(0);
        Ok(FilterCounts {
            time_class: self.scope.time_class.as_str().to_owned(),
            games_searched: n(games),
            matching_moves: n(moves),
            matching_games: n(hit_games),
        })
    }

    async fn examples(&self, q: ExamplesQuery) -> anyhow::Result<Vec<PositionRecord>> {
        let limit = q.limit.clamp(1, MAX_EXAMPLES) as usize;
        if let Some(f) = &q.filter {
            let want = i64::try_from(limit * 2).unwrap_or(i64::MAX);
            let mut out = Vec::new();
            for (game_id, ply) in self.filter_refs(f, want).await? {
                if out.len() >= limit {
                    break;
                }
                let Ok(ply) = u32::try_from(ply) else {
                    continue;
                };
                if let Some(p) = self.position(&game_id, ply).await? {
                    out.push(p);
                }
            }
            return Ok(out);
        }
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

    fn count_examples<'a>(
        &'a self,
        filter: &'a MoveFilter,
    ) -> BoxFuture<'a, anyhow::Result<Option<FilterCounts>>> {
        Box::pin(async move { Ok(Some(self.filter_counts(filter).await?)) })
    }

    fn resolve_anchor<'a>(
        &'a self,
        anchor: &'a ChatAnchor,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(self.anchor(anchor))
    }
}

/// Where a chat turn's coaching input came from (ADR 0086).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    /// A build-profile snapshot (`coaching_inputs.id`).
    Snapshot(Uuid),
    /// A report run's input (`report_runs.id`).
    ReportRun(Uuid),
}

/// A stored coaching input and where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredInput {
    pub input: CoachingInput,
    pub source: InputSource,
}

impl StoredInput {
    /// The `coaching_inputs` row, when the input was a build-profile snapshot.
    pub fn snapshot_id(&self) -> Option<Uuid> {
        match self.source {
            InputSource::Snapshot(id) => Some(id),
            InputSource::ReportRun(_) => None,
        }
    }
}

/// The parsed input of a stored row; `None` (logged without content) when it no longer
/// validates or belongs to another account.
fn checked(raw: &Value, account_id: Uuid, what: &'static str) -> Option<CoachingInput> {
    match parse_coaching_input(raw) {
        Ok(i) if i.account_id == account_id => Some(i),
        Ok(_) => {
            tracing::warn!(
                what,
                "stored coaching input belongs to another account; ignored"
            );
            None
        }
        Err(issues) => {
            tracing::warn!(
                what,
                issues = issues.len(),
                "stored coaching input is invalid; ignored"
            );
            None
        }
    }
}

/// The most current coaching input of (account, time class) (ADR 0086): of the newest
/// build-profile snapshot and the newest report run's input, the one of the newer contract
/// version (`inputVersion`), then the later `generatedAt`. Every build-profile writes a snapshot,
/// so the coach is as current as the profile, while reports are throttled. `None` when neither
/// exists (or validates).
pub async fn latest_coaching_input(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> anyhow::Result<Option<StoredInput>> {
    let snapshot: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, input FROM coaching_inputs WHERE account_id = $1          AND time_class = $2::time_class ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(pool)
    .await?;
    let report: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, input FROM report_runs WHERE account_id = $1 AND time_class = $2::time_class          AND input IS NOT NULL ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(pool)
    .await?;
    let snapshot = snapshot.and_then(|(id, raw)| {
        checked(&raw, account_id, "snapshot").map(|input| StoredInput {
            input,
            source: InputSource::Snapshot(id),
        })
    });
    let report = report.and_then(|(id, raw)| {
        checked(&raw, account_id, "report").map(|input| StoredInput {
            input,
            source: InputSource::ReportRun(id),
        })
    });
    let rank = |s: &StoredInput| (version_rank(s.input.input_version), s.input.generated_at);
    Ok(match (snapshot, report) {
        // A tie goes to the snapshot: it is the profile's own.
        (Some(s), Some(r)) => Some(if rank(&r) > rank(&s) { r } else { s }),
        (s, r) => s.or(r),
    })
}

fn version_rank(v: InputVersion) -> u8 {
    match v {
        InputVersion::V1 => 1,
        InputVersion::V2 => 2,
    }
}
