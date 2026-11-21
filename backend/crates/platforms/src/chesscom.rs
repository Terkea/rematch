//! chess.com importer: monthly archives walked newest first, normalised into `NormalizedGame`.

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

use chess_core::TimeClass;
use chess_core::time_control::classify_time_control;
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use regex::Regex;
use serde_json::Value;
use time::{Date, Month, OffsetDateTime, Time};

use crate::types::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};

const DEFAULT_BASE: &str = "https://api.chess.com/pub/player";

static ARCHIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/(\d{4})/(\d{2})/?$").expect("valid regex"));
static DAILY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^1/(\d+)$").expect("valid regex"));
static LIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)(?:\+(\d+))?$").expect("valid regex"));
static MOVE_WORD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.").expect("valid regex"));

/// Opening names whose real spelling contains a hyphen that the ECOUrl slug cannot distinguish
/// from a word separator.
const HYPHENATED: &[&str] = &["Caro-Kann", "Nimzo-Indian", "Bogo-Indian", "Semi-Slav"];

/// Value of PGN header `name`, unescaped for `\"` and `\\`.
fn header(pgn: &str, name: &str) -> Option<String> {
    let prefix = format!("[{name} \"");
    for line in pgn.lines() {
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => return Some(out),
                _ => out.push(c),
            }
        }
        return None;
    }
    None
}

fn to_result(pgn: &str) -> String {
    match header(pgn, "Result").as_deref() {
        Some(r @ ("1-0" | "0-1" | "1/2-1/2")) => r.to_string(),
        _ => "*".to_string(),
    }
}

/// `(class, base, increment)`; `None` when the time control cannot be parsed.
fn timing(tc: &str) -> Option<(TimeClass, i32, i32)> {
    if let Some(c) = DAILY_RE.captures(tc) {
        let base: i32 = c[1].parse().ok()?;
        return Some((TimeClass::Daily, base, 0));
    }
    let c = LIVE_RE.captures(tc)?;
    let base: i32 = c[1].parse().ok()?;
    let inc: i32 = c.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
    Some((
        classify_time_control(base.unsigned_abs(), inc.unsigned_abs()),
        base,
        inc,
    ))
}

/// Opening name from the ECOUrl slug, dropping the trailing move list. Known hyphenated names
/// (Caro-Kann, Nimzo-Indian, ...) keep their hyphen.
fn name_from_slug(slug: &str) -> Option<String> {
    let words: Vec<&str> = slug.split('-').collect();
    let cut = words
        .iter()
        .position(|w| MOVE_WORD_RE.is_match(w))
        .unwrap_or(words.len());
    let words = &words[..cut];
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if i + 1 < words.len() {
            let pair = format!("{}-{}", words[i], words[i + 1]);
            if HYPHENATED.iter().any(|h| h.eq_ignore_ascii_case(&pair)) {
                parts.push(pair);
                i += 2;
                continue;
            }
        }
        parts.push(words[i].to_string());
        i += 1;
    }
    let name = parts.join(" ");
    (!name.is_empty()).then_some(name)
}

/// Prefers the PGN `Opening` header, else derives the name from the `ECOUrl` slug.
fn opening_name(pgn: &str) -> Option<String> {
    if let Some(o) = header(pgn, "Opening") {
        let o = o.trim();
        if !o.is_empty() {
            return Some(o.to_string());
        }
    }
    let url = header(pgn, "ECOUrl")?;
    name_from_slug(url.rsplit('/').next()?)
}

fn player(v: &Value) -> Option<Player> {
    Some(Player {
        name: v.get("username")?.as_str()?.to_string(),
        rating: v
            .get("rating")
            .and_then(Value::as_i64)
            .and_then(|r| i32::try_from(r).ok()),
    })
}

fn end_time(raw: &Value) -> Option<i64> {
    let v = raw.get("end_time")?;
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

/// Normalises one chess.com archive game. `None` for variants (`rules` other than `chess`),
/// malformed games, and games with an unparseable `time_control` (which is logged).
pub fn normalize_chesscom(raw: &Value) -> Option<NormalizedGame> {
    if let Some(rules) = raw.get("rules").and_then(Value::as_str)
        && !rules.is_empty()
        && rules != "chess"
    {
        return None;
    }
    let url = raw.get("url")?.as_str()?;
    let pgn = raw.get("pgn")?.as_str()?;
    let tc = raw.get("time_control")?.as_str()?;
    let Some((time_class, base, inc)) = timing(tc) else {
        tracing::warn!("chesscom: skipping game with unparseable time_control \"{tc}\": {url}");
        return None;
    };
    let played_at = OffsetDateTime::from_unix_timestamp(end_time(raw)?).ok()?;
    let platform_game_id = url
        .split('/')
        .rfind(|s| !s.is_empty())
        .unwrap_or(url)
        .to_string();
    Some(NormalizedGame {
        platform: Platform::Chesscom,
        platform_game_id,
        pgn: pgn.to_string(),
        played_at,
        time_class,
        base_seconds: Some(base),
        increment_seconds: Some(inc),
        rated: raw.get("rated").and_then(Value::as_bool).unwrap_or(false),
        white: player(raw.get("white")?)?,
        black: player(raw.get("black")?)?,
        result: to_result(pgn),
        termination: header(pgn, "Termination"),
        eco: header(pgn, "ECO"),
        opening_name: opening_name(pgn),
        lichess_evals: None,
    })
}

/// Milliseconds at UTC midnight on the 1st of `month`; month 13 rolls into January next year.
fn month_start_ms(year: i32, month: u8) -> Option<i128> {
    let (y, m) = if month > 12 {
        (year + 1, 1)
    } else {
        (year, month)
    };
    let d = Date::from_calendar_date(y, Month::try_from(m).ok()?, 1).ok()?;
    Some(i128::from(d.with_time(Time::MIDNIGHT).assume_utc().unix_timestamp()) * 1000)
}

fn ms(t: OffsetDateTime) -> i128 {
    t.unix_timestamp_nanos().div_euclid(1_000_000)
}

type Archive = (String, Option<(i32, u8)>);

/// Archive URLs sorted newest first by the year/month in the URL (not by API order);
/// URLs without a parseable month keep their relative order after the dated ones.
fn sorted_archives(archives: &[String]) -> Vec<Archive> {
    let mut v: Vec<Archive> = archives
        .iter()
        .map(|a| {
            let ym = ARCHIVE_RE.captures(a).and_then(|c| {
                let y: i32 = c[1].parse().ok()?;
                let m: u8 = c[2].parse().ok()?;
                (1..=12).contains(&m).then_some((y, m))
            });
            (a.clone(), ym)
        })
        .collect();
    v.sort_by(|a, b| match (a.1, b.1) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    v
}

struct State {
    client: Arc<dyn PlatformClient>,
    archives: VecDeque<Archive>,
    buffer: VecDeque<NormalizedGame>,
    opts: FetchOptions,
    yielded: u32,
    done: bool,
}

async fn next_game(mut s: State) -> Result<Option<(NormalizedGame, State)>, PlatformError> {
    loop {
        if s.done {
            return Ok(None);
        }
        if let Some(g) = s.buffer.pop_front() {
            s.yielded += 1;
            if s.opts.max.is_some_and(|m| s.yielded >= m) {
                s.done = true;
            }
            return Ok(Some((g, s)));
        }
        let Some((url, ym)) = s.archives.pop_front() else {
            return Ok(None);
        };
        let since = s.opts.since.map(ms);
        let until = s.opts.until.map(ms);
        if let Some((y, m)) = ym {
            if let Some(since) = since
                && month_start_ms(y, m + 1).is_some_and(|end| end <= since)
            {
                return Ok(None); // this and every older month ends before `since`
            }
            if let Some(until) = until
                && month_start_ms(y, m).is_some_and(|start| start > until)
            {
                continue; // the whole month is after `until`
            }
        }
        let body = s.client.get_json(&url, vec![]).await?;
        let mut games: Vec<&Value> = body
            .get("games")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        games.sort_by_key(|g| std::cmp::Reverse(end_time(g).unwrap_or(i64::MIN)));
        for g in games {
            let Some(end) = end_time(g) else { continue };
            let end_ms = i128::from(end) * 1000;
            if since.is_some_and(|t| end_ms < t) || until.is_some_and(|t| end_ms > t) {
                continue;
            }
            if let Some(n) = normalize_chesscom(g) {
                s.buffer.push_back(n);
            }
        }
    }
}

/// chess.com game source.
#[derive(Clone, Debug)]
pub struct ChesscomSource {
    base: String,
}

impl Default for ChesscomSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ChesscomSource {
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE)
    }

    /// Override the `.../pub/player` base URL (used by tests against a local server).
    pub fn with_base_url(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
        }
    }
}

impl GameSource for ChesscomSource {
    fn fetch<'a>(
        &'a self,
        client: Arc<dyn PlatformClient>,
        username: &'a str,
        opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        if opts.max == Some(0) {
            return stream::empty().boxed();
        }
        let url = format!(
            "{}/{}/games/archives",
            self.base,
            urlencode(&username.to_lowercase())
        );
        stream::once(async move {
            let body = client.get_json(&url, vec![]).await?;
            let archives: Vec<String> = body
                .get("archives")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Ok::<_, PlatformError>(State {
                client,
                archives: sorted_archives(&archives).into(),
                buffer: VecDeque::new(),
                opts,
                yielded: 0,
                done: false,
            })
        })
        .map(|r| match r {
            Ok(state) => stream::try_unfold(state, next_game).boxed(),
            Err(e) => stream::once(async move { Err(e) }).boxed(),
        })
        .flatten()
        .boxed()
    }
}

/// Percent-encodes like JS `encodeURIComponent`.
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_keeps_known_hyphenated_names() {
        assert_eq!(
            name_from_slug("Caro-Kann-Defense-Advance-Variation-3.e5-c5").as_deref(),
            Some("Caro-Kann Defense Advance Variation")
        );
        assert_eq!(
            name_from_slug("Nimzo-Indian-Defense-Rubinstein-Variation").as_deref(),
            Some("Nimzo-Indian Defense Rubinstein Variation")
        );
        assert_eq!(
            name_from_slug("Sicilian-Defense").as_deref(),
            Some("Sicilian Defense")
        );
        assert_eq!(name_from_slug("1.e4-e5"), None);
    }

    #[test]
    fn opening_header_wins_over_slug() {
        let pgn = "[Opening \"Caro-Kann Defense: Advance\"]\n[ECOUrl \"https://www.chess.com/openings/Caro-Kann-Defense\"]\n\n1. e4 *";
        assert_eq!(
            opening_name(pgn).as_deref(),
            Some("Caro-Kann Defense: Advance")
        );
    }
}
