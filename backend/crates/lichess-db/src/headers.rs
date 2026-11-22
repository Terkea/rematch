//! PGN header parsing (R2 Task 19) for Lichess dump games.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time};

use crate::download::DumpError;
use crate::movetext::movetext_offset;

/// The header fields the pipeline needs from one dump game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpHeader {
    /// `Site`, e.g. `https://lichess.org/Fx1aB2cD`.
    pub site: String,
    /// The last path segment of `Site`.
    pub lichess_game_id: String,
    pub white: String,
    pub black: String,
    /// `None` for `?` (unrated or unknown).
    pub white_elo: Option<i32>,
    pub black_elo: Option<i32>,
    /// Raw `TimeControl`: `"180+2"`, or `"-"` for correspondence.
    pub time_control: String,
    /// `None` unless `TimeControl` is `<base>+<increment>`.
    pub base_seconds: Option<u32>,
    pub increment_seconds: Option<u32>,
    pub result: String,
    pub termination: Option<String>,
    /// `UTCDate` + `UTCTime`.
    pub utc: OffsetDateTime,
    /// The movetext has a `[%eval` comment.
    pub has_eval: bool,
    /// The movetext has a `[%clk` comment.
    pub has_clock: bool,
    /// `WhiteTitle` / `BlackTitle`, e.g. `"GM"` or `"BOT"`.
    pub white_title: Option<String>,
    pub black_title: Option<String>,
    /// `WhiteRatingDiff` / `BlackRatingDiff`: the rating change from this game.
    pub white_rating_diff: Option<i32>,
    pub black_rating_diff: Option<i32>,
}

/// Rating change at or above which a side looks provisional (see [`looks_provisional`]).
pub const PROVISIONAL_DIFF: i32 = 25;

/// Heuristic, since the dump has no provisional flag: a rating change of at least
/// [`PROVISIONAL_DIFF`] points in one game. Under Glicko-2 an even game moves an established
/// rating (RD about 60) by about 10 points and even a large upset by about 20, while a provisional
/// one (RD at least 110, the Lichess threshold) moves by about 30 in an even game. `None` (no
/// `RatingDiff` tag) is never flagged.
pub fn looks_provisional(rating_diff: Option<i32>) -> bool {
    rating_diff.is_some_and(|d| d.saturating_abs() >= PROVISIONAL_DIFF)
}

impl DumpHeader {
    /// Either side has the `BOT` title.
    pub fn has_bot(&self) -> bool {
        [&self.white_title, &self.black_title]
            .iter()
            .any(|t| t.as_deref() == Some("BOT"))
    }

    pub fn white_provisional(&self) -> bool {
        looks_provisional(self.white_rating_diff)
    }

    pub fn black_provisional(&self) -> bool {
        looks_provisional(self.black_rating_diff)
    }
}

#[derive(Default)]
struct Tags {
    site: Option<String>,
    white: Option<String>,
    black: Option<String>,
    white_elo: Option<String>,
    black_elo: Option<String>,
    result: Option<String>,
    utc_date: Option<String>,
    utc_time: Option<String>,
    time_control: Option<String>,
    termination: Option<String>,
    white_title: Option<String>,
    black_title: Option<String>,
    white_rating_diff: Option<String>,
    black_rating_diff: Option<String>,
}

fn decode(msg: impl Into<String>) -> DumpError {
    DumpError::Decode(msg.into())
}

fn clip(s: &str) -> &str {
    match s.char_indices().nth(80) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Parses `[Name "value"]` (with `\"` and `\\` escapes). `None` when malformed.
fn parse_tag(line: &str) -> Option<(&str, String)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (name, rest) = inner.split_once(|c: char| c.is_ascii_whitespace())?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let body = rest.trim_start().strip_prefix('"')?;
    let mut value = String::with_capacity(body.len());
    let mut chars = body.char_indices();
    loop {
        match chars.next()? {
            (_, '\\') => match chars.next()? {
                (_, c @ ('"' | '\\')) => value.push(c),
                (_, c) => {
                    value.push('\\');
                    value.push(c);
                }
            },
            (i, '"') => {
                return body[i + 1..].trim().is_empty().then_some((name, value));
            }
            (_, c) => value.push(c),
        }
    }
}

fn parse_time_control(tc: &str) -> (Option<u32>, Option<u32>) {
    match tc.split_once('+') {
        Some((base, inc)) => match (base.parse(), inc.parse()) {
            (Ok(base), Ok(inc)) => (Some(base), Some(inc)),
            _ => (None, None),
        },
        None => (None, None),
    }
}

fn three<T: std::str::FromStr>(s: &str, sep: char) -> Option<(T, T, T)> {
    let mut parts = s.split(sep);
    let a = parts.next()?.parse().ok()?;
    let b = parts.next()?.parse().ok()?;
    let c = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((a, b, c))
}

/// `UTCDate` `YYYY.MM.DD` + `UTCTime` `HH:MM:SS`.
fn parse_utc(date: &str, clock: &str) -> Option<OffsetDateTime> {
    let (y, m, d): (i32, u8, u8) = {
        let (y, m, d) = three::<u32>(date, '.')?;
        (
            i32::try_from(y).ok()?,
            u8::try_from(m).ok()?,
            u8::try_from(d).ok()?,
        )
    };
    let (h, min, s) = three::<u8>(clock, ':')?;
    let date = Date::from_calendar_date(y, Month::try_from(m).ok()?, d).ok()?;
    let clock = Time::from_hms(h, min, s).ok()?;
    Some(PrimitiveDateTime::new(date, clock).assume_utc())
}

/// The last path segment of `Site` (`https://lichess.org/<id>`), alphanumeric, ≤ 64 chars.
fn game_id(site: &str) -> Option<&str> {
    let (_, tail) = site.rsplit_once('/')?;
    let id = tail.split(['?', '#']).next()?;
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then_some(id)
}

/// `"+6"`, `"-6"` or `"0"`.
fn rating_diff(v: &str) -> Option<i32> {
    v.strip_prefix('+').unwrap_or(v).parse().ok()
}

fn required(tag: Option<String>, name: &str) -> Result<String, DumpError> {
    tag.ok_or_else(|| decode(format!("missing [{name}] header")))
}

/// Parses the headers of one dump game. When a tag repeats, the last one wins (as in
/// `chess_core::pgn`). `Site`, `White`, `Black`, `Result`, `UTCDate`, `UTCTime` and
/// `TimeControl` are required; a malformed tag line is a `Decode` error.
pub fn parse_dump_headers(pgn: &str) -> Result<DumpHeader, DumpError> {
    let mut tags = Tags::default();
    let mut seen = false;
    for line in pgn.lines() {
        let line = line.trim();
        if line.is_empty() {
            if seen {
                break;
            }
            continue;
        }
        if !line.starts_with('[') {
            break;
        }
        seen = true;
        let (name, value) =
            parse_tag(line).ok_or_else(|| decode(format!("malformed header {:?}", clip(line))))?;
        let slot = match name {
            "Site" => &mut tags.site,
            "White" => &mut tags.white,
            "Black" => &mut tags.black,
            "WhiteElo" => &mut tags.white_elo,
            "BlackElo" => &mut tags.black_elo,
            "Result" => &mut tags.result,
            "UTCDate" => &mut tags.utc_date,
            "UTCTime" => &mut tags.utc_time,
            "TimeControl" => &mut tags.time_control,
            "Termination" => &mut tags.termination,
            "WhiteTitle" => &mut tags.white_title,
            "BlackTitle" => &mut tags.black_title,
            "WhiteRatingDiff" => &mut tags.white_rating_diff,
            "BlackRatingDiff" => &mut tags.black_rating_diff,
            _ => continue,
        };
        *slot = Some(value);
    }

    let site = required(tags.site, "Site")?;
    let lichess_game_id = game_id(&site)
        .ok_or_else(|| decode(format!("no game id in Site {:?}", clip(&site))))?
        .to_owned();
    let utc_date = required(tags.utc_date, "UTCDate")?;
    let utc_time = required(tags.utc_time, "UTCTime")?;
    let utc = parse_utc(&utc_date, &utc_time)
        .ok_or_else(|| decode(format!("bad UTCDate/UTCTime {utc_date:?} {utc_time:?}")))?;
    let time_control = required(tags.time_control, "TimeControl")?;
    let (base_seconds, increment_seconds) = parse_time_control(&time_control);
    let movetext = &pgn[movetext_offset(pgn)..];

    Ok(DumpHeader {
        site,
        lichess_game_id,
        white: required(tags.white, "White")?,
        black: required(tags.black, "Black")?,
        white_elo: tags.white_elo.and_then(|v| v.parse().ok()),
        black_elo: tags.black_elo.and_then(|v| v.parse().ok()),
        time_control,
        base_seconds,
        increment_seconds,
        result: required(tags.result, "Result")?,
        termination: tags.termination,
        utc,
        has_eval: movetext.contains("[%eval"),
        has_clock: movetext.contains("[%clk"),
        white_title: tags.white_title,
        black_title: tags.black_title,
        white_rating_diff: tags.white_rating_diff.as_deref().and_then(rating_diff),
        black_rating_diff: tags.black_rating_diff.as_deref().and_then(rating_diff),
    })
}
