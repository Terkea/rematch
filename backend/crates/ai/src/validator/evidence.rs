//! The evidence index: every fact the narrative may cite, taken from the `CoachingInput` (and, in
//! chat, from tool results). A port of m3's `validator/evidence.ts` (the index and the path
//! index); rendering lives in [`super::format`] and [`super::segments_render`].
//!
//! The walk reads [`CoachingInput::to_json`], the wire JSON TS walks. `serde_json` maps are
//! sorted, so the walk visits object keys in key order where TS uses insertion order. That only
//! changes the order of [`Evidence::numbers`] (a set in TS; never rely on its order) and which of
//! two writes to one path wins, which the recorded TS index (`tests/evidence.rs`) shows never
//! happens on the fixtures. R3 adds engine results (`{{eng:}}`), and chat-tool writers
//! (`add_line`, `put_number_path`, `put_date_path`).

use std::collections::{BTreeMap, HashMap, HashSet};

use chess_core::Color;
use serde_json::Value;

use super::bounded::is_js_space;
use super::extract::{extract_dates, extract_numbers};
use super::pv::{fen_after, pv_to_san, strip_san};
use crate::input::types::{CoachingInput, ExemplarPosition, MoveRef, Score};

/// What the narrative may say about one position (`"{gameId}#{ply}"`).
#[derive(Clone, Debug, PartialEq)]
pub struct PositionEvidence {
    pub game_id: String,
    pub ply: u32,
    pub fen: String,
    /// SAN, each also without its `+#!?` suffix.
    pub allowed_san: HashSet<String>,
    pub allowed_uci: HashSet<String>,
    pub played: Option<MoveRef>,
    pub best: Option<MoveRef>,
    /// SAN of the legal prefix of the best line (from `fen`).
    pub best_pv_san: Vec<String>,
    /// SAN of the legal prefix of the refutation line (from the position after `played`).
    pub refutation_pv_san: Vec<String>,
}

/// An engine line given as UCI from a FEN.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PvFrom {
    pub from_fen: String,
    pub uci: Vec<String>,
}

/// The lines `{{pv:…:best|refutation:n}}` read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PvSides {
    pub best: Option<PvFrom>,
    pub refutation: Option<PvFrom>,
}

/// TS `addPosition`'s argument.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PositionArgs {
    pub game_id: String,
    pub ply: u32,
    pub fen: String,
    pub played: Option<MoveRef>,
    pub best: Option<MoveRef>,
    /// Lines whose legal moves are allowed (SAN and UCI).
    pub pvs: Vec<PvFrom>,
    pub pv_sides: PvSides,
}

/// One line of an interactive engine result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineLine {
    /// White's point of view.
    pub score: Score,
    /// The side to move in the analysed position (carried for the chat engine; the rendered
    /// evaluation is from White's point of view).
    pub side_to_move: Color,
    pub san: Vec<String>,
}

/// An interactive engine result, cited with `{{eng:<evalId>:…}}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineEvidence {
    /// The moves the player tried, in SAN.
    pub tried_san: Vec<String>,
    /// Lines 1..=3, best first.
    pub lines: Vec<EngineLine>,
}

/// The evidence index. Build it with [`create_evidence`] or [`build_evidence`].
#[derive(Clone, Debug, Default)]
pub struct Evidence {
    pub positions: HashMap<String, PositionEvidence>,
    pub game_ids: HashSet<String>,
    /// SAN allowed for each opening line (each also without its suffix).
    pub lines: HashMap<String, HashSet<String>>,
    /// Ordered raw SAN of each line (`movesSan`), for `{{line:}}` tokens.
    pub line_moves: HashMap<String, Vec<String>>,
    /// Dotted paths for `{{num:}}` tokens.
    pub number_paths: BTreeMap<String, f64>,
    /// Dotted paths for `{{date:}}` tokens (`YYYY-MM-DD`).
    pub date_paths: BTreeMap<String, String>,
    /// Numbers the text may state, each once (JS `Set` semantics: 0 and -0 are one value).
    /// Not in TS order.
    pub numbers: Vec<f64>,
    pub ratios: Vec<f64>,
    /// `YYYY-MM-DD` and `MM-DD`.
    pub dates: HashSet<String>,
    /// R3: interactive engine results by eval id.
    pub engine_evals: HashMap<String, EngineEvidence>,
    /// R3 (Task 8 fix 1): number paths that hold a rate or share in [0, 1], the only paths a
    /// `pct0`/`pct1` token may format.
    pub rate_paths: HashSet<String>,
    /// R3 (Task 8 fix 1): the path of every exemplar object in the input, mapped to its
    /// position key `gameId#ply` (`weaknesses.W1.exemplars.0` → `li-w1-0#5`).
    pub path_positions: HashMap<String, String>,
    num_seen: HashSet<u64>,
    ratio_seen: HashSet<u64>,
}

/// JS SameValueZero key for a finite number.
fn same_value_zero(n: f64) -> u64 {
    if n == 0.0 { 0 } else { n.to_bits() }
}

/// TS `Number(n.toPrecision(12))`.
fn clean(n: f64) -> f64 {
    if !n.is_finite() {
        return n;
    }
    format!("{n:.11e}").parse().unwrap_or(n)
}

fn add_san(set: &mut HashSet<String>, san: &str) {
    set.insert(san.to_owned());
    set.insert(strip_san(san));
}

const MINED_KEYS: [&str; 5] = ["label", "description", "name", "before", "after"];
const SKIP_KEYS: [&str; 5] = ["bestPv", "refutationPv", "movesSan", "tags", "fen"];

/// TS `ARRAY_KEYS`: top-level arrays whose elements are keyed by a field instead of an index.
fn array_id_field(key: &str) -> Option<&'static str> {
    match key {
        "areaScores" => Some("area"),
        "metrics" => Some("key"),
        "weaknesses" => Some("id"),
        "openings" => Some("lineKey"),
        "changes" => Some("seriesKey"),
        _ => None,
    }
}

/// TS `/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// TS `/^(\d{4})-(\d{2})-(\d{2})(?:$|T)/` (ASCII digits): year, month, day.
fn iso_prefix(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() < 10 || !(b.len() == 10 || b[10] == b'T') {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        b[r.clone()]
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| s[r].parse().ok())?
    };
    if b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y = digits(0..4)?;
    Some((i32::try_from(y).ok()?, digits(5..7)?, digits(8..10)?))
}

/// TS `seg`: `[.|:\s]` (JS `\s`) become `_`.
fn seg(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '.' | '|' | ':') || is_js_space(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `path` with its suffix `from` replaced by `to`; unchanged when it does not end with `from`.
fn with_suffix(path: &str, from: &str, to: &str) -> String {
    match path.strip_suffix(from) {
        Some(head) => format!("{head}{to}"),
        None => path.to_owned(),
    }
}

/// An empty index (TS `createEvidence`).
pub fn create_evidence() -> Evidence {
    Evidence::default()
}

impl Evidence {
    pub fn add_game(&mut self, game_id: &str) {
        self.game_ids.insert(game_id.to_owned());
    }

    /// Adds a position, or merges allowed moves into the one already at `"{gameId}#{ply}"`
    /// (its `fen`, `played` and `best` stay the first ones). Never panics on bad lines.
    pub fn add_position(&mut self, p: PositionArgs) {
        self.game_ids.insert(p.game_id.clone());
        let key = format!("{}#{}", p.game_id, p.ply);
        let pe = self
            .positions
            .entry(key)
            .or_insert_with(|| PositionEvidence {
                game_id: p.game_id.clone(),
                ply: p.ply,
                fen: p.fen.clone(),
                allowed_san: HashSet::new(),
                allowed_uci: HashSet::new(),
                played: p.played.clone(),
                best: p.best.clone(),
                best_pv_san: Vec::new(),
                refutation_pv_san: Vec::new(),
            });
        for m in [&p.played, &p.best].into_iter().flatten() {
            add_san(&mut pe.allowed_san, &m.san);
            pe.allowed_uci.insert(m.uci.clone());
        }
        if let Some(b) = &p.pv_sides.best {
            pe.best_pv_san = pv_to_san(&b.from_fen, &b.uci);
        }
        if let Some(r) = &p.pv_sides.refutation {
            pe.refutation_pv_san = pv_to_san(&r.from_fen, &r.uci);
        }
        for pv in &p.pvs {
            let sans = pv_to_san(&pv.from_fen, &pv.uci);
            for s in &sans {
                add_san(&mut pe.allowed_san, s);
            }
            for u in &pv.uci[..sans.len()] {
                pe.allowed_uci.insert(u.clone());
            }
        }
    }

    /// Adds a finite ratio once.
    pub fn add_ratio(&mut self, r: f64) {
        if r.is_finite() && self.ratio_seen.insert(same_value_zero(r)) {
            self.ratios.push(r);
        }
    }

    fn add_num(&mut self, n: f64) {
        if n.is_finite() && self.num_seen.insert(same_value_zero(n)) {
            self.numbers.push(n);
        }
    }

    fn add_date(&mut self, year: Option<i32>, month: u32, day: u32) {
        self.dates.insert(format!("{month:02}-{day:02}"));
        if let Some(y) = year {
            self.dates.insert(format!("{y}-{month:02}-{day:02}"));
            self.add_num(f64::from(y));
        }
        self.add_num(f64::from(month));
        self.add_num(f64::from(day));
    }

    /// Mines numbers and dates from a JSON value (TS `addNumbersFrom`): every number (plus
    /// derived forms by key), array lengths, ISO date prefixes, and numbers and dates in the text
    /// of `label`, `description`, `name`, `before` and `after`. UUID strings are never mined.
    pub fn add_numbers_from(&mut self, value: &Value) {
        self.walk_numbers(value, None);
    }

    fn walk_numbers(&mut self, v: &Value, key: Option<&str>) {
        match v {
            Value::Number(n) => {
                let Some(v) = n.as_f64() else { return };
                self.add_num(v);
                if (0.0..=1.0).contains(&v) {
                    self.add_num(clean(v * 100.0));
                }
                match key {
                    Some("cp") => {
                        self.add_num(clean(v / 100.0));
                        self.add_num(clean(-v / 100.0));
                    }
                    Some("mate") => {
                        self.add_num(v.abs());
                        self.add_num(-v.abs());
                    }
                    Some("ply" | "deviationPly") => self.add_num((v / 2.0).ceil()),
                    Some("baseSeconds") => self.add_num(clean(v / 60.0)),
                    _ => {}
                }
            }
            Value::String(s) => {
                if is_uuid(s) {
                    return;
                }
                if let Some((y, m, d)) = iso_prefix(s) {
                    self.add_date(Some(y), m, d);
                }
                if key.is_some_and(|k| MINED_KEYS.contains(&k)) {
                    for n in extract_numbers(s) {
                        self.add_num(n.value);
                    }
                    for d in extract_dates(s) {
                        self.add_date(d.year, d.month, d.day);
                    }
                }
            }
            Value::Array(xs) => {
                self.add_num(xs.len() as f64);
                for x in xs {
                    self.walk_numbers(x, key);
                }
            }
            Value::Object(o) => {
                for (k, x) in o {
                    self.walk_numbers(x, Some(k));
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }

    /// R3 (chat tools): indexes a line's SAN (each also without its suffix) and keeps its order
    /// for `{{line:}}`. Replaces a line already under `key`, as TS's `Map.set` does.
    pub fn add_line(&mut self, key: &str, moves: Vec<String>) {
        let mut set = HashSet::new();
        for m in &moves {
            add_san(&mut set, m);
        }
        self.lines.insert(key.to_owned(), set);
        self.line_moves.insert(key.to_owned(), moves);
    }

    /// R3 (chat tools): a `{{num:}}` path. Like TS's `putN`, the first finite value wins.
    pub fn put_number_path(&mut self, path: &str, n: f64) {
        if n.is_finite() && !self.number_paths.contains_key(path) {
            self.number_paths.insert(path.to_owned(), n);
        }
    }

    /// R3 (chat tools): a `{{date:}}` path (`YYYY-MM-DD`). The first value wins.
    pub fn put_date_path(&mut self, path: &str, date: String) {
        self.date_paths.entry(path.to_owned()).or_insert(date);
    }

    /// R3: an interactive engine result for `{{eng:<evalId>:…}}`. Replaces one with the same id.
    pub fn add_engine_eval(&mut self, eval_id: String, e: EngineEvidence) {
        self.engine_evals.insert(eval_id, e);
    }

    fn add_exemplar(&mut self, e: &ExemplarPosition) {
        let after = fen_after(&e.fen, &e.played.uci);
        let best = PvFrom {
            from_fen: e.fen.clone(),
            uci: e.best_pv.clone(),
        };
        let refutation = after.map(|from_fen| PvFrom {
            from_fen,
            uci: e.refutation_pv.clone(),
        });
        let mut pvs = vec![best.clone()];
        pvs.extend(refutation.clone());
        self.add_position(PositionArgs {
            game_id: e.game_id.clone(),
            ply: e.ply,
            fen: e.fen.clone(),
            played: Some(e.played.clone()),
            best: e.best.clone(),
            pvs,
            pv_sides: PvSides {
                best: Some(best),
                refutation,
            },
        });
    }

    /// TS `indexPaths`: the `{{num:}}` and `{{date:}}` path index.
    fn index_paths(&mut self, input: &CoachingInput, json: &Value) {
        self.walk_paths(json, "", None);
        for c in &input.changes {
            let base = format!("changes.{}", seg(&c.series_key));
            if c.rate_before != 0.0 {
                self.put_number_path(
                    &format!("{base}.ratioAfterBefore"),
                    c.rate_after / c.rate_before,
                );
            }
            if c.rate_after != 0.0 {
                self.put_number_path(
                    &format!("{base}.ratioBeforeAfter"),
                    c.rate_before / c.rate_after,
                );
            }
        }
    }

    fn walk_paths(&mut self, v: &Value, path: &str, key: Option<&str>) {
        match v {
            Value::Number(n) => {
                let Some(v) = n.as_f64() else { return };
                self.put_number_path(path, v);
                match key {
                    Some("cp") => {
                        self.put_number_path(&with_suffix(path, "cp", "pawns"), clean(v / 100.0));
                    }
                    // TS `path.replace(/(ply|deviationPly)$/i, …)`: the path ends with the key
                    // (or with an array index, when nothing is replaced).
                    Some("ply") => self
                        .put_number_path(&with_suffix(path, "ply", "moveNumber"), (v / 2.0).ceil()),
                    Some("deviationPly") => self.put_number_path(
                        &with_suffix(path, "deviationPly", "deviationMoveNumber"),
                        (v / 2.0).ceil(),
                    ),
                    Some("baseSeconds") => self.put_number_path(
                        &with_suffix(path, "baseSeconds", "baseMinutes"),
                        clean(v / 60.0),
                    ),
                    _ => {}
                }
            }
            Value::String(s) => {
                if iso_prefix(s).is_some() {
                    self.put_date_path(path, s[..10].to_owned());
                }
            }
            Value::Array(xs) => {
                if key.is_some_and(|k| SKIP_KEYS.contains(&k)) {
                    return;
                }
                self.put_number_path(&format!("{path}.count"), xs.len() as f64);
                let id_field = key.filter(|_| !path.contains('.')).and_then(array_id_field);
                for (i, x) in xs.iter().enumerate() {
                    let id = id_field.and_then(|f| x.get(f)).and_then(Value::as_str);
                    let child = match id {
                        Some(id) => format!("{path}.{}", seg(id)),
                        None => format!("{path}.{i}"),
                    };
                    self.walk_paths(x, &child, key);
                }
            }
            Value::Object(o) => {
                if let (Some(Value::String(g)), Some(Value::Number(p)), true) =
                    (o.get("gameId"), o.get("ply"), o.contains_key("fen"))
                {
                    self.path_positions
                        .entry(path.to_owned())
                        .or_insert_with(|| format!("{g}#{p}"));
                }
                for (k, x) in o {
                    if SKIP_KEYS.contains(&k.as_str()) {
                        continue;
                    }
                    let child = if path.is_empty() {
                        seg(k)
                    } else {
                        format!("{path}.{}", seg(k))
                    };
                    self.walk_paths(x, &child, Some(k));
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }
}

impl Evidence {
    /// Marks a number path as a rate or share in [0, 1] (`pct0`/`pct1` may format it).
    pub fn add_rate_path(&mut self, path: &str) {
        self.rate_paths.insert(path.to_owned());
    }

    /// The position key of the exemplar whose object contains `path`, if any.
    pub fn position_of_path(&self, path: &str) -> Option<&str> {
        let mut p = path;
        loop {
            if let Some(k) = self.path_positions.get(p) {
                return Some(k);
            }
            p = &p[..p.rfind('.')?];
        }
    }

    /// The rates and shares of the input (R3 Task 8 fix 1).
    fn index_rate_paths(&mut self, input: &CoachingInput) {
        use crate::input::types::MetricUnit;
        let add = |ev: &mut Evidence, p: String| {
            if ev
                .number_paths
                .get(&p)
                .is_some_and(|v| (0.0..=1.0).contains(v))
            {
                ev.rate_paths.insert(p);
            }
        };
        for w in &input.weaknesses {
            let b = format!("weaknesses.{}", seg(&w.id));
            for f in [
                "posteriorRate",
                "intervalLow",
                "intervalHigh",
                "bandMedian",
                "confidence",
                "pImp",
            ] {
                add(self, format!("{b}.{f}"));
            }
        }
        for c in &input.changes {
            let b = format!("changes.{}", seg(&c.series_key));
            for f in ["rateBefore", "rateAfter", "posteriorProb"] {
                add(self, format!("{b}.{f}"));
            }
        }
        for o in &input.openings {
            let b = format!("openings.{}", seg(&o.line_key));
            add(self, format!("{b}.score"));
            for (i, _) in o.alternatives.iter().enumerate() {
                add(self, format!("{b}.alternatives.{i}.bandScore"));
            }
        }
        for m in input.metrics.iter().filter(|m| m.unit == MetricUnit::Rate) {
            let b = format!("metrics.{}", seg(&m.key));
            for f in ["value", "low", "high", "bandMedian"] {
                add(self, format!("{b}.{f}"));
            }
        }
    }
}

/// The full index for a report (TS `buildEvidence`).
pub fn build_evidence(input: &CoachingInput) -> Evidence {
    let mut ev = create_evidence();
    let json = input.to_json();
    ev.add_numbers_from(&json);
    ev.index_paths(input, &json);
    ev.index_rate_paths(input);
    for w in &input.weaknesses {
        for e in &w.exemplars {
            ev.add_exemplar(e);
        }
    }
    for s in &input.strengths {
        for e in &s.exemplars {
            ev.add_exemplar(e);
        }
    }
    for o in &input.openings {
        for e in &o.examples {
            ev.add_exemplar(e);
        }
        let mut line = HashSet::new();
        for m in &o.moves_san {
            add_san(&mut line, m);
        }
        if let Some(d) = &o.deviation_move_san {
            add_san(&mut line, d);
        }
        for a in &o.alternatives {
            for m in &a.moves_san {
                add_san(&mut line, m);
            }
        }
        ev.lines.insert(o.line_key.clone(), line);
        ev.line_moves
            .insert(o.line_key.clone(), o.moves_san.clone());
    }
    for c in &input.changes {
        ev.add_game(&c.onset_game_id);
        for e in &c.examples {
            ev.add_exemplar(&e.position);
        }
        if c.rate_before != 0.0 {
            ev.add_ratio(c.rate_after / c.rate_before);
        }
        if c.rate_after != 0.0 {
            ev.add_ratio(c.rate_before / c.rate_after);
        }
    }
    ev
}

pub fn resolve_number_path(ev: &Evidence, path: &str) -> Option<f64> {
    ev.number_paths.get(path).copied()
}

pub fn resolve_date_path<'a>(ev: &'a Evidence, path: &str) -> Option<&'a str> {
    ev.date_paths.get(path).map(String::as_str)
}

/// Every `{{num:}}` path, sorted.
pub fn list_number_paths(ev: &Evidence) -> Vec<String> {
    ev.number_paths.keys().cloned().collect()
}

/// Every `{{date:}}` path, sorted.
pub fn list_date_paths(ev: &Evidence) -> Vec<String> {
    ev.date_paths.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_keeps_twelve_significant_digits_like_to_precision() {
        assert_eq!(clean(0.1 + 0.2), 0.3);
        assert_eq!(clean(-160.0 / 100.0), -1.6);
        assert_eq!(clean(0.07 * 100.0), 7.0);
        assert_eq!(clean(600.0 / 60.0), 10.0);
        assert_eq!(clean(1.0 / 3.0), 0.333333333333);
    }

    #[test]
    fn iso_prefix_and_uuid_follow_the_ts_patterns() {
        assert_eq!(iso_prefix("2026-11-21"), Some((2026, 11, 21)));
        assert_eq!(iso_prefix("2026-11-21T10:00:00.000Z"), Some((2026, 11, 21)));
        assert_eq!(iso_prefix("2026-11-21 10:00"), None);
        assert_eq!(iso_prefix("2026-1-21"), None);
        assert_eq!(iso_prefix("２０２６-11-21"), None);
        assert_eq!(iso_prefix("2026-11-2é"), None);
        assert!(is_uuid("4A3F9C1E-2b7d-4e8a-9f10-3c5d6e7f8a9b"));
        assert!(!is_uuid("4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9"));
        assert!(!is_uuid("4a3f9c1e_2b7d-4e8a-9f10-3c5d6e7f8a9b"));
    }

    #[test]
    fn seg_replaces_dots_bars_colons_and_js_spaces() {
        assert_eq!(seg("a.b|c:d e\u{a0}f\u{3000}g"), "a_b_c_d_e_f_g");
    }
}
