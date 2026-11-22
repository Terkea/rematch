//! `CoachingInput::validate`: every check of the M3 zod schema, in Rust (ADR-R2-2).
//!
//! The types already fix the shape (strict keys, enums, integer types). This module checks the
//! rest: zod's bounds (string lengths in Unicode code points, array sizes, number ranges,
//! patterns, finite numbers) and the refinements JSON Schema cannot express (FEN validity, side to
//! move vs ply, unit ranges, `low <= high`, the posterior inside its interval, events <=
//! opportunities, unique ranks, each area exactly once, onset order, analysed <= imported,
//! `windowStart <= windowEnd`). Every issue is reported, not only the first. Paths are zod's,
//! joined with `.` as `parseCoachingInput` does (`weaknesses.0.exemplars.0.fen`).

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

use chess_core::board::Position;
use regex::Regex;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, UtcOffset};
use uuid::Uuid;

use crate::enums::AreaId;
use crate::limits::MIN_COLOR_GAMES;
use crate::limits::{
    ECO_MAX_CHARS, FEN_MAX_CHARS, LIMITS, MAX_BASE_SECONDS, MAX_COUNT, MAX_CP, MAX_EXCESS_EPL,
    MAX_INCREMENT_SECONDS, MAX_MATE, MAX_PLY, MAX_PRIORITY, MAX_RATING, UCI_PATTERN, UUID_PATTERN,
    WEAKNESS_ID_MAX_CHARS, WEAKNESS_ID_PATTERN,
};
use crate::types::{
    Alternative, AreaScore, Basis, Benchmark, BenchmarkSource, ByColor, CandidateCause, CauseValue,
    CoachingInput, ColorMetric, ColorSummary, ColorWeakness, ExemplarPosition, InputVersion,
    Metric, MetricUnit, MoveRef, OpeningVerdict, RankedWeakness, Score, Strength, TimelineChange,
};

/// One problem, as `parseCoachingInput` reports it: `{ path, message }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl CoachingInput {
    /// Checks every zod bound and refinement; returns all issues found.
    pub fn validate(&self) -> Result<(), Vec<Issue>> {
        let mut c = Checker::default();
        c.input(self);
        if c.issues.is_empty() {
            Ok(())
        } else {
            Err(c.issues)
        }
    }
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("contract patterns are valid regexes")
}

static UCI_RE: LazyLock<Regex> = LazyLock::new(|| regex(UCI_PATTERN));
static WEAKNESS_ID_RE: LazyLock<Regex> = LazyLock::new(|| regex(WEAKNESS_ID_PATTERN));
static UUID_RE: LazyLock<Regex> = LazyLock::new(|| regex(UUID_PATTERN));

fn join(prefix: &str, segment: impl fmt::Display) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

/// String length as zod 4.6 measures it: Unicode code points (`util.codePointLength`, which
/// counts a surrogate pair once), the same unit as JSON Schema's `maxLength`. A Rust `str` holds
/// no lone surrogates, so this is `chars().count()`.
fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Structural FEN rules that `Position::from_fen` alone does not enforce (it accepts `44` as a
/// rank): six non-empty fields and eight ranks of pieces and digits 1-8 that fill eight squares
/// with no two digits in a row, as chess.js `validateFen` requires. The whitespace rule (fields
/// separated by single spaces, no other whitespace) is deliberately stricter than chess.js/zod
/// (which accept runs of whitespace): it is the safe direction, and the builder never emits such
/// FENs.
fn fen_shape_ok(fen: &str) -> bool {
    let fields: Vec<&str> = fen.split(' ').collect();
    if fields.len() != 6
        || fields
            .iter()
            .any(|f| f.is_empty() || f.chars().any(char::is_whitespace))
    {
        return false;
    }
    let ranks: Vec<&str> = fields[0].split('/').collect();
    ranks.len() == 8
        && ranks.iter().all(|rank| {
            let mut squares = 0;
            let mut previous_digit = false;
            for c in rank.chars() {
                match c.to_digit(10) {
                    Some(d @ 1..=8) if !previous_digit => {
                        squares += d;
                        previous_digit = true;
                    }
                    None if "prnbqkPRNBQK".contains(c) => {
                        squares += 1;
                        previous_digit = false;
                    }
                    _ => return false,
                }
            }
            squares == 8
        })
}

#[derive(Default)]
struct Checker {
    issues: Vec<Issue>,
}

impl Checker {
    fn push(&mut self, path: String, message: impl Into<String>) {
        self.issues.push(Issue {
            path,
            message: message.into(),
        });
    }

    // ---- zod primitives --------------------------------------------------------------------

    fn string(&mut self, path: String, s: &str, min: usize, max: usize) {
        let len = char_len(s);
        if len < min {
            self.push(
                path,
                format!("Too small: expected string to have >={min} characters"),
            );
        } else if len > max {
            self.push(
                path,
                format!("Too big: expected string to have <={max} characters"),
            );
        }
    }

    fn pattern(&mut self, path: String, s: &str, re: &Regex) {
        if !re.is_match(s) {
            self.push(
                path,
                format!("Invalid string: must match pattern /{}/", re.as_str()),
            );
        }
    }

    fn items(&mut self, path: String, len: usize, max: usize) {
        if len > max {
            self.push(
                path,
                format!("Too big: expected array to have <={max} items"),
            );
        }
    }

    /// A finite number (zod `z.number()` rejects NaN and infinities).
    fn finite(&mut self, path: String, v: f64) -> bool {
        if v.is_finite() {
            true
        } else {
            self.push(
                path,
                format!("Invalid input: expected a finite number, got {v}"),
            );
            false
        }
    }

    fn number(&mut self, path: String, v: f64, min: f64, max: f64) {
        if !self.finite(path.clone(), v) {
            return;
        }
        if v < min {
            self.push(path, format!("Too small: expected number to be >={min}"));
        } else if v > max {
            self.push(path, format!("Too big: expected number to be <={max}"));
        }
    }

    /// zod `Unit`: 0..1.
    fn unit(&mut self, path: String, v: f64) {
        self.number(path, v, 0.0, 1.0);
    }

    /// zod `Pct`: 0..100.
    fn pct(&mut self, path: String, v: f64) {
        self.number(path, v, 0.0, 100.0);
    }

    fn int(&mut self, path: String, v: i64, min: i64, max: i64) {
        if v < min {
            self.push(path, format!("Too small: expected number to be >={min}"));
        } else if v > max {
            self.push(path, format!("Too big: expected number to be <={max}"));
        }
    }

    /// zod `Count`: 0..=100 000 000 (`u32` already excludes negatives).
    fn count(&mut self, path: String, v: u32) {
        self.int(path, v.into(), 0, MAX_COUNT.into());
    }

    fn ply(&mut self, path: String, v: u32) {
        self.int(path, v.into(), 1, MAX_PLY.into());
    }

    fn label(&mut self, path: String, s: &str) {
        self.string(path, s, 1, LIMITS.label_chars);
    }

    fn id(&mut self, path: String, s: &str) {
        self.string(path, s, 1, LIMITS.id_chars);
    }

    fn san(&mut self, path: String, s: &str) {
        self.string(path, s, 1, LIMITS.san_chars);
    }

    fn san_line(&mut self, path: &str, line: &[String]) {
        self.items(path.to_owned(), line.len(), LIMITS.moves_san);
        for (i, san) in line.iter().enumerate() {
            self.san(join(path, i), san);
        }
    }

    fn uci_line(&mut self, path: &str, line: &[String]) {
        self.items(path.to_owned(), line.len(), LIMITS.pv_plies);
        for (i, uci) in line.iter().enumerate() {
            self.pattern(join(path, i), uci, &UCI_RE);
        }
    }

    fn weakness_id(&mut self, path: String, s: &str) {
        self.string(path.clone(), s, 0, WEAKNESS_ID_MAX_CHARS);
        self.pattern(path, s, &WEAKNESS_ID_RE);
    }

    /// zod `z.iso.datetime().max(40)`: `common::js_time::iso` always writes the 24-character
    /// `YYYY-MM-DDTHH:mm:ss.sssZ`, so only a year outside 0000..=9999 can break the pattern.
    fn datetime(&mut self, path: String, t: OffsetDateTime) {
        if !(0..=9999).contains(&t.to_offset(UtcOffset::UTC).year()) {
            self.push(path, "Invalid ISO datetime");
        }
    }

    /// zod `z.iso.date()`: `YYYY-MM-DD` with a four-digit year.
    fn date(&mut self, path: String, d: Date) {
        if !(0..=9999).contains(&d.year()) {
            self.push(path, "Invalid ISO date");
        }
    }

    fn uuid(&mut self, path: String, u: &Uuid) {
        if !UUID_RE.is_match(&u.hyphenated().to_string()) {
            self.push(path, "Invalid UUID");
        }
    }

    fn score(&mut self, path: &str, s: &Score) {
        match *s {
            Score::Cp(cp) => self.int(join(path, "cp"), cp.into(), (-MAX_CP).into(), MAX_CP.into()),
            Score::Mate(m) => self.int(
                join(path, "mate"),
                m.into(),
                (-MAX_MATE).into(),
                MAX_MATE.into(),
            ),
        }
    }

    fn move_ref(&mut self, path: &str, m: &MoveRef) {
        self.san(join(path, "san"), &m.san);
        self.pattern(join(path, "uci"), &m.uci, &UCI_RE);
    }

    // ---- objects ---------------------------------------------------------------------------

    fn input(&mut self, c: &CoachingInput) {
        // `inputVersion`, `platform`, `timeClass` and `band` are enums: always valid.
        self.uuid("accountId".into(), &c.account_id);
        if let Some(tc) = &c.time_control {
            self.int(
                "timeControl.baseSeconds".into(),
                tc.base_seconds.into(),
                0,
                MAX_BASE_SECONDS.into(),
            );
            self.int(
                "timeControl.incrementSeconds".into(),
                tc.increment_seconds.into(),
                0,
                MAX_INCREMENT_SECONDS.into(),
            );
        }
        if let Some(rating) = c.rating {
            self.int("rating".into(), rating.into(), 0, MAX_RATING.into());
        }
        self.datetime("generatedAt".into(), c.generated_at);
        self.basis("basis", &c.basis);

        self.items("areaScores".into(), c.area_scores.len(), AreaId::ALL.len());
        for (i, s) in c.area_scores.iter().enumerate() {
            self.area_score(&join("areaScores", i), s);
        }
        let once = |a: AreaId| c.area_scores.iter().filter(|s| s.area == a).count() == 1;
        if c.area_scores.len() != AreaId::ALL.len() || !AreaId::ALL.into_iter().all(once) {
            let names: Vec<&str> = AreaId::ALL.iter().map(|a| a.as_str()).collect();
            self.push(
                "areaScores".into(),
                format!(
                    "areaScores must contain each of {} exactly once",
                    names.join(", ")
                ),
            );
        }

        self.items("metrics".into(), c.metrics.len(), LIMITS.metrics);
        for (i, m) in c.metrics.iter().enumerate() {
            self.metric(&join("metrics", i), m);
        }

        self.items("weaknesses".into(), c.weaknesses.len(), LIMITS.weaknesses);
        for (i, w) in c.weaknesses.iter().enumerate() {
            self.weakness(&join("weaknesses", i), w);
        }
        let mut ranks = HashSet::new();
        for (i, w) in c.weaknesses.iter().enumerate() {
            if !ranks.insert(w.rank) {
                self.push(
                    join(&join("weaknesses", i), "rank"),
                    format!("duplicate rank {}", w.rank),
                );
            }
        }

        self.items("strengths".into(), c.strengths.len(), LIMITS.strengths);
        for (i, s) in c.strengths.iter().enumerate() {
            self.strength(&join("strengths", i), s);
        }

        self.items("openings".into(), c.openings.len(), LIMITS.openings);
        for (i, o) in c.openings.iter().enumerate() {
            self.opening(&join("openings", i), o);
        }

        self.items("changes".into(), c.changes.len(), LIMITS.changes);
        for (i, ch) in c.changes.iter().enumerate() {
            self.change(&join("changes", i), ch);
        }

        // v2 (ADR 0084): `byColor` is required, `benchmark` optional; v1 has neither.
        match c.input_version {
            InputVersion::V1 => {
                if c.by_color.is_some() {
                    self.push("byColor".into(), "byColor needs coaching-input-v2");
                }
                if c.benchmark.is_some() {
                    self.push("benchmark".into(), "benchmark needs coaching-input-v2");
                }
            }
            InputVersion::V2 => match &c.by_color {
                Some(b) => self.by_color("byColor", b),
                None => self.push(
                    "byColor".into(),
                    "Invalid input: expected object, received undefined",
                ),
            },
        }
        if let Some(b) = &c.benchmark {
            self.benchmark("benchmark", b);
        }
    }

    fn benchmark(&mut self, p: &str, b: &Benchmark) {
        for (field, v) in [("ratingLow", b.rating_low), ("ratingHigh", b.rating_high)] {
            if let Some(v) = v {
                self.int(join(p, field), v.into(), 0, MAX_RATING.into());
            }
        }
        for (field, v) in [("players", b.players), ("games", b.games)] {
            if let Some(v) = v {
                self.count(join(p, field), v);
            }
        }
        if b.source == BenchmarkSource::Peers
            && (b.rating_low.is_none()
                || b.rating_high.is_none()
                || b.players.is_none()
                || b.games.is_none())
        {
            self.push(
                p.to_owned(),
                "a peers benchmark states ratingLow, ratingHigh, players and games",
            );
        }
        if let (Some(lo), Some(hi)) = (b.rating_low, b.rating_high)
            && lo > hi
        {
            self.push(join(p, "ratingLow"), "ratingLow must be <= ratingHigh");
        }
    }

    fn by_color(&mut self, p: &str, b: &ByColor) {
        self.color_summary(&join(p, "white"), &b.white);
        self.color_summary(&join(p, "black"), &b.black);
    }

    fn color_summary(&mut self, p: &str, s: &ColorSummary) {
        self.count(join(p, "games"), s.games);
        for (field, v) in [("wins", s.wins), ("draws", s.draws), ("losses", s.losses)] {
            self.count(join(p, field), v);
        }
        if u64::from(s.wins) + u64::from(s.draws) + u64::from(s.losses) != u64::from(s.games) {
            self.push(join(p, "games"), "wins + draws + losses must equal games");
        }
        if s.sufficient != (s.games >= MIN_COLOR_GAMES) {
            self.push(
                join(p, "sufficient"),
                format!("sufficient must be games >= {MIN_COLOR_GAMES}"),
            );
        }
        self.need_more(p, s.sufficient, s.need_more);
        self.color_metric(&join(p, "score"), &s.score, MetricUnit::Rate);
        self.color_metric(&join(p, "accuracy"), &s.accuracy, MetricUnit::Pct);
        self.color_metric(
            &join(p, "blundersPerGame"),
            &s.blunders_per_game,
            MetricUnit::PerGame,
        );
        if s.score.n != s.games {
            self.push(join(p, "score.n"), "score.n must equal games");
        }
        if !s.sufficient {
            for (field, m) in [
                ("score", &s.score),
                ("accuracy", &s.accuracy),
                ("blundersPerGame", &s.blunders_per_game),
            ] {
                if m.sufficient {
                    self.push(
                        join(&join(p, field), "sufficient"),
                        "a colour without enough games states no number",
                    );
                }
            }
        }
        self.items(
            join(p, "weaknesses"),
            s.weaknesses.len(),
            LIMITS.color_weaknesses,
        );
        let mut keys = HashSet::new();
        for (i, w) in s.weaknesses.iter().enumerate() {
            let wp = join(&join(p, "weaknesses"), i);
            self.color_weakness(&wp, w);
            if !keys.insert(w.series_key.as_str()) {
                self.push(
                    join(&wp, "seriesKey"),
                    format!("duplicate seriesKey {}", w.series_key),
                );
            }
        }
    }

    /// `needMore` is set exactly when not sufficient.
    fn need_more(&mut self, p: &str, sufficient: bool, need: Option<u32>) {
        if let Some(need) = need {
            self.count(join(p, "needMore"), need);
        }
        match (sufficient, need) {
            (true, Some(_)) => {
                self.push(join(p, "needMore"), "needMore must be null when sufficient")
            }
            (false, None) => self.push(
                join(p, "needMore"),
                "needMore is required when not sufficient",
            ),
            _ => {}
        }
    }

    fn color_metric(&mut self, p: &str, m: &ColorMetric, unit: MetricUnit) {
        if m.unit != unit {
            self.push(
                join(p, "unit"),
                format!("unit must be \"{}\"", unit.as_str()),
            );
        }
        self.count(join(p, "n"), m.n);
        self.need_more(p, m.sufficient, m.need_more);
        // Insufficient evidence is stated as null, never as a number (ADR 0084).
        if !m.sufficient {
            for (field, v) in [("value", m.value), ("low", m.low), ("high", m.high)] {
                if v.is_some() {
                    self.push(
                        join(p, field),
                        format!("{field} must be null when not sufficient"),
                    );
                }
            }
        } else if m.value.is_none() {
            self.push(join(p, "value"), "value is required when sufficient");
        }
        let max = m.unit.max();
        for (field, v) in [("value", m.value), ("low", m.low), ("high", m.high)] {
            if let Some(v) = v
                && self.finite(join(p, field), v)
                && !(0.0..=max).contains(&v)
            {
                self.push(
                    join(p, field),
                    format!(
                        "{field} {v} is outside 0..{max} for unit \"{}\"",
                        m.unit.as_str()
                    ),
                );
            }
        }
        if m.low.is_some() != m.high.is_some() {
            self.push(join(p, "low"), "low and high are both null or both set");
        }
        if let (Some(low), Some(high)) = (m.low, m.high)
            && low > high
        {
            self.push(join(p, "low"), "low must be <= high");
        }
        if let (Some(v), Some(low), Some(high)) = (m.value, m.low, m.high)
            && (v < low || v > high)
        {
            self.push(join(p, "value"), "value must lie within [low, high]");
        }
    }

    fn color_weakness(&mut self, p: &str, w: &ColorWeakness) {
        self.id(join(p, "seriesKey"), &w.series_key);
        self.weakness_id(join(p, "id"), &w.id);
        if let Some(variant) = &w.variant {
            self.string(join(p, "variant"), variant, 1, LIMITS.tag_chars);
        }
        self.label(join(p, "label"), &w.label);
        self.unit(join(p, "rate"), w.rate);
        self.unit(join(p, "low"), w.low);
        self.unit(join(p, "high"), w.high);
        self.count(join(p, "events"), w.events);
        self.count(join(p, "opportunities"), w.opportunities);
        if let Some(r) = w.peer_rate {
            self.unit(join(p, "peerRate"), r);
        }
        if let Some(pc) = w.peer_percentile {
            self.pct(join(p, "peerPercentile"), pc);
        }
        if let Some(n) = w.peer_players {
            self.count(join(p, "peerPlayers"), n);
        }
        if w.peer_rate.is_none() && (w.peer_percentile.is_some() || w.peer_players.is_some()) {
            self.push(
                join(p, "peerRate"),
                "peerPercentile and peerPlayers need a peerRate",
            );
        }
        if w.low > w.high {
            self.push(join(p, "low"), "low must be <= high");
        }
        if w.rate < w.low || w.rate > w.high {
            self.push(join(p, "rate"), "rate must lie within [low, high]");
        }
        if w.events > w.opportunities {
            self.push(join(p, "events"), "events must be <= opportunities");
        }
    }

    fn basis(&mut self, p: &str, b: &Basis) {
        self.count(join(p, "analysedGames"), b.analysed_games);
        self.count(join(p, "importedGames"), b.imported_games);
        self.date(join(p, "windowStart"), b.window_start);
        self.date(join(p, "windowEnd"), b.window_end);
        if b.analysed_games > b.imported_games {
            self.push(
                join(p, "analysedGames"),
                "analysedGames must be <= importedGames",
            );
        }
        if b.window_start > b.window_end {
            self.push(join(p, "windowStart"), "windowStart must be <= windowEnd");
        }
    }

    fn area_score(&mut self, p: &str, s: &AreaScore) {
        if let Some(score) = s.score {
            self.pct(join(p, "score"), score);
        }
        self.count(join(p, "n"), s.n);
        if let Some(need) = s.need_more {
            self.count(join(p, "needMore"), need);
        }
    }

    fn metric(&mut self, p: &str, m: &Metric) {
        self.id(join(p, "key"), &m.key);
        self.label(join(p, "label"), &m.label);
        let in_unit = [
            ("value", Some(m.value)),
            ("low", m.low),
            ("high", m.high),
            ("bandMedian", m.band_median),
        ];
        self.finite(join(p, "value"), m.value);
        self.count(join(p, "n"), m.n);
        for (field, v) in &in_unit[1..] {
            if let Some(v) = *v {
                self.finite(join(p, field), v);
            }
        }
        if let Some(bp) = m.band_percentile {
            self.pct(join(p, "bandPercentile"), bp);
        }
        if let Some(need) = m.need_more {
            self.count(join(p, "needMore"), need);
        }

        // Refinement: value, low, high and bandMedian lie in the unit's range (a non-finite
        // value is already reported above).
        let max = m.unit.max();
        for (field, v) in in_unit {
            if let Some(v) = v
                && v.is_finite()
                && !(0.0..=max).contains(&v)
            {
                self.push(
                    join(p, field),
                    format!(
                        "{field} {v} is outside 0..{max} for unit \"{}\"",
                        m.unit.as_str()
                    ),
                );
            }
        }
        if let (Some(low), Some(high)) = (m.low, m.high)
            && low > high
        {
            self.push(join(p, "low"), "low must be <= high");
        }
    }

    fn weakness(&mut self, p: &str, w: &RankedWeakness) {
        self.weakness_id(join(p, "id"), &w.id);
        if let Some(variant) = &w.variant {
            self.string(join(p, "variant"), variant, 1, LIMITS.tag_chars);
        }
        self.label(join(p, "label"), &w.label);
        self.int(join(p, "rank"), w.rank.into(), 1, LIMITS.weaknesses as i64);
        self.unit(join(p, "posteriorRate"), w.posterior_rate);
        self.unit(join(p, "intervalLow"), w.interval_low);
        self.unit(join(p, "intervalHigh"), w.interval_high);
        self.unit(join(p, "bandMedian"), w.band_median);
        self.pct(join(p, "bandPercentile"), w.band_percentile);
        self.number(
            join(p, "excessEplPerGame"),
            w.excess_epl_per_game,
            0.0,
            MAX_EXCESS_EPL.into(),
        );
        self.unit(join(p, "confidence"), w.confidence);
        self.number(join(p, "priority"), w.priority, 0.0, MAX_PRIORITY.into());
        if let Some(p_imp) = w.p_imp {
            self.unit(join(p, "pImp"), p_imp);
        }
        self.count(join(p, "events"), w.events);
        self.count(join(p, "opportunities"), w.opportunities);
        self.exemplars(
            &join(p, "exemplars"),
            &w.exemplars,
            LIMITS.exemplars_per_item,
        );

        if w.interval_low > w.interval_high {
            self.push(
                join(p, "intervalLow"),
                "intervalLow must be <= intervalHigh",
            );
        }
        if w.posterior_rate < w.interval_low || w.posterior_rate > w.interval_high {
            self.push(
                join(p, "posteriorRate"),
                "posteriorRate must lie within [intervalLow, intervalHigh]",
            );
        }
        if w.events > w.opportunities {
            self.push(join(p, "events"), "events must be <= opportunities");
        }
    }

    fn strength(&mut self, p: &str, s: &Strength) {
        self.label(join(p, "label"), &s.label);
        if let Some(key) = &s.metric_key {
            self.id(join(p, "metricKey"), key);
        }
        self.exemplars(
            &join(p, "exemplars"),
            &s.exemplars,
            LIMITS.exemplars_per_item,
        );
    }

    fn opening(&mut self, p: &str, o: &OpeningVerdict) {
        self.id(join(p, "lineKey"), &o.line_key);
        self.label(join(p, "name"), &o.name);
        if let Some(eco) = &o.eco {
            self.string(join(p, "eco"), eco, 1, ECO_MAX_CHARS);
        }
        self.san_line(&join(p, "movesSan"), &o.moves_san);
        self.count(join(p, "games"), o.games);
        self.unit(join(p, "score"), o.score);
        if let Some(perf) = o.performance_vs_expected {
            self.number(join(p, "performanceVsExpected"), perf, -1.0, 1.0);
        }
        if let Some(score) = &o.avg_eval_at_opening_end {
            self.score(&join(p, "avgEvalAtOpeningEnd"), score);
        }
        if let Some(ply) = o.deviation_ply {
            self.ply(join(p, "deviationPly"), ply);
        }
        if let Some(san) = &o.deviation_move_san {
            self.san(join(p, "deviationMoveSan"), san);
        }
        let alternatives = join(p, "alternatives");
        self.items(
            alternatives.clone(),
            o.alternatives.len(),
            LIMITS.alternatives,
        );
        for (i, a) in o.alternatives.iter().enumerate() {
            self.alternative(&join(&alternatives, i), a);
        }
        self.exemplars(
            &join(p, "examples"),
            &o.examples,
            LIMITS.examples_per_opening,
        );
    }

    fn alternative(&mut self, p: &str, a: &Alternative) {
        self.label(join(p, "name"), &a.name);
        self.san_line(&join(p, "movesSan"), &a.moves_san);
        self.unit(join(p, "bandScore"), a.band_score);
        self.count(join(p, "games"), a.games);
    }

    fn change(&mut self, p: &str, c: &TimelineChange) {
        self.id(join(p, "seriesKey"), &c.series_key);
        if let Some(id) = &c.weakness_id {
            self.weakness_id(join(p, "weaknessId"), id);
        }
        self.label(join(p, "label"), &c.label);
        self.id(join(p, "onsetGameId"), &c.onset_game_id);
        self.date(join(p, "onsetDate"), c.onset_date);
        self.date(join(p, "onsetFrom"), c.onset_from);
        self.date(join(p, "onsetTo"), c.onset_to);
        self.unit(join(p, "rateBefore"), c.rate_before);
        self.unit(join(p, "rateAfter"), c.rate_after);
        self.unit(join(p, "posteriorProb"), c.posterior_prob);
        let causes = join(p, "candidateCauses");
        self.items(
            causes.clone(),
            c.candidate_causes.len(),
            LIMITS.candidate_causes,
        );
        for (i, cause) in c.candidate_causes.iter().enumerate() {
            self.cause(&join(&causes, i), cause);
        }
        let examples = join(p, "examples");
        self.items(
            examples.clone(),
            c.examples.len(),
            LIMITS.examples_per_change,
        );
        for (i, e) in c.examples.iter().enumerate() {
            self.exemplar(&join(&examples, i), &e.position);
        }
        if c.onset_from > c.onset_date || c.onset_date > c.onset_to {
            self.push(
                join(p, "onsetDate"),
                "must satisfy onsetFrom <= onsetDate <= onsetTo",
            );
        }
    }

    fn cause(&mut self, p: &str, c: &CandidateCause) {
        self.string(
            join(p, "description"),
            &c.description,
            1,
            LIMITS.description_chars,
        );
        for (field, v) in [("before", &c.before), ("after", &c.after)] {
            match v {
                CauseValue::Text(s) => self.string(join(p, field), s, 0, LIMITS.label_chars),
                CauseValue::Number(n) => {
                    self.finite(join(p, field), *n);
                }
            }
        }
    }

    fn exemplars(&mut self, p: &str, list: &[ExemplarPosition], max: usize) {
        self.items(p.to_owned(), list.len(), max);
        for (i, e) in list.iter().enumerate() {
            self.exemplar(&join(p, i), e);
        }
    }

    /// zod `exemplarFields` plus `checkSideToMove` (shared by `ExemplarPosition` and the
    /// flattened `ChangeExample`).
    fn exemplar(&mut self, p: &str, e: &ExemplarPosition) {
        self.id(join(p, "gameId"), &e.game_id);
        self.ply(join(p, "ply"), e.ply);
        self.datetime(join(p, "playedAt"), e.played_at);
        self.string(join(p, "fen"), &e.fen, 0, FEN_MAX_CHARS);
        if !fen_shape_ok(&e.fen) || Position::from_fen(&e.fen).is_err() {
            self.push(join(p, "fen"), "invalid FEN");
        }
        self.move_ref(&join(p, "played"), &e.played);
        if let Some(best) = &e.best {
            self.move_ref(&join(p, "best"), best);
        }
        self.uci_line(&join(p, "bestPv"), &e.best_pv);
        self.uci_line(&join(p, "refutationPv"), &e.refutation_pv);
        let tags = join(p, "tags");
        self.items(tags.clone(), e.tags.len(), LIMITS.tags);
        for (i, tag) in e.tags.iter().enumerate() {
            self.string(join(&tags, i), tag, 1, LIMITS.tag_chars);
        }
        self.score(&join(p, "evalBefore"), &e.eval_before);
        self.score(&join(p, "evalAfter"), &e.eval_after);
        self.pct(join(p, "winBefore"), e.win_before);
        self.pct(join(p, "winAfter"), e.win_after);

        // checkSideToMove reads the FEN's second field as zod does, valid FEN or not.
        if let Some(turn @ ("w" | "b")) = e.fen.split(' ').nth(1) {
            let expected = if e.ply % 2 == 1 { "w" } else { "b" };
            if turn != expected {
                self.push(
                    join(p, "fen"),
                    format!(
                        "FEN side to move \"{turn}\" disagrees with ply {} (expected \"{expected}\")",
                        e.ply
                    ),
                );
            }
        }
    }
}
