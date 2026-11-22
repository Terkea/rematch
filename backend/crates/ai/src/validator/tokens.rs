//! Reference tokens (ADR-AI-8): narrative text never states chess facts directly; it points at them.
//! A port of m3's `validator/tokens.ts`, plus the R3 tokens `{{eng:…}}` (interactive engine
//! results) and `{{offtopic}}` (the chat's off-topic marker).
//!
//! Deviation 2 (ADR-R3-2): `Token.index` and a token error's `index` are byte offsets in the
//! canonical text ([`canonical_for_tokens`]); a stray-brace error's `index` is a byte offset in
//! `stripped`, where TS reports it too. None of the patterns here need a lookaround.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use super::bounded::scan;
use super::normalize::{NormMode, strip_invisibles_with};
pub use crate::segments::PvSide;

/// How a `{{num:<path>|<fmt>}}` token renders its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumFormat {
    Int,
    Pct0,
    Pct1,
    Dec1,
    Dec2,
    Signed1,
}

impl NumFormat {
    /// Every format, in TS's `NUM_FORMATS` order.
    pub const ALL: [NumFormat; 6] = [
        NumFormat::Int,
        NumFormat::Pct0,
        NumFormat::Pct1,
        NumFormat::Dec1,
        NumFormat::Dec2,
        NumFormat::Signed1,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NumFormat::Int => "int",
            NumFormat::Pct0 => "pct0",
            NumFormat::Pct1 => "pct1",
            NumFormat::Dec1 => "dec1",
            NumFormat::Dec2 => "dec2",
            NumFormat::Signed1 => "signed1",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == s)
    }
}

/// What a token points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenBody {
    /// `{{move:<gameId>:<ply>}}`: the move played at that ply.
    Move { game_id: String, ply: u32 },
    /// `{{best:<gameId>:<ply>}}`: the engine's best move at that ply.
    Best { game_id: String, ply: u32 },
    /// `{{pv:<gameId>:<ply>:best|refutation:<n>}}`: the n-th move of that engine line.
    Pv {
        game_id: String,
        ply: u32,
        side: PvSide,
        n: u32,
    },
    /// `{{line:<lineKey>[:<n>]}}`: an opening line, or its n-th move.
    Line { line_key: String, n: Option<u32> },
    /// `{{num:<path>[|<fmt>]}}`: a number from the input.
    Num {
        path: String,
        fmt: Option<NumFormat>,
    },
    /// `{{date:<path>}}`: a date from the input.
    Date { path: String },
    /// `{{sq:<square>}}`: a square name.
    Sq { square: String },
    /// R3: `{{eng:<evalId>:tried}}` (`line` and `n` None: the tried moves),
    /// `{{eng:<evalId>:<line>}}` (that line's evaluation, line 1 to 3) and
    /// `{{eng:<evalId>:<line>:<n>}}` (the n-th SAN move of the line). `evalId` is 32 lowercase hex.
    Eng {
        eval_id: String,
        line: Option<u32>,
        n: Option<u32>,
    },
    /// R3: `{{offtopic}}`, the chat's off-topic marker. It takes no arguments.
    Offtopic,
}

impl TokenBody {
    /// The token kind as written (TS's `kind`).
    pub fn kind(&self) -> &'static str {
        match self {
            TokenBody::Move { .. } => "move",
            TokenBody::Best { .. } => "best",
            TokenBody::Pv { .. } => "pv",
            TokenBody::Line { .. } => "line",
            TokenBody::Num { .. } => "num",
            TokenBody::Date { .. } => "date",
            TokenBody::Sq { .. } => "sq",
            TokenBody::Eng { .. } => "eng",
            TokenBody::Offtopic => "offtopic",
        }
    }
}

/// A parsed token: its body, its source text, and its byte offset in the canonical text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub body: TokenBody,
    pub raw: String,
    pub index: usize,
}

/// A malformed token or a stray brace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TokenSyntaxError {
    pub message: String,
    pub raw: String,
    pub index: usize,
}

/// What each token is replaced with in `stripped`.
pub const TOKEN_PLACEHOLDER: char = '§';

const STRAY_BRACE: &str = "stray or unterminated brace";

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid regex")
}

// TS `\d` has no `u` flag: ASCII digits only (deviation 2). JS `$` without `m` is the text end.
static ID_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z0-9_.-]+$"));
static PLY_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[1-9][0-9]{0,3}$"));
static N_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[1-9][0-9]{0,2}$"));
static PATH_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z0-9_#-]+(?:\.[A-Za-z0-9_#-]+)*$"));
static LINE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z0-9_.:-]+$"));
static SQUARE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[a-h][1-8]$"));
static EVAL_ID_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[0-9a-f]{32}$"));
static ENG_LINE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[1-3]$"));
static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| re(r"\{\{([^{}]*)\}\}"));
static BRACES_RE: LazyLock<Regex> = LazyLock::new(|| re(r"[{}]+"));

/// The text tokens are parsed from: invisible characters stripped, NFKC applied, stripped again.
/// Renderers must use the same form. It uses TS's exact invisible set, not the hardened one, so
/// `stripped` keeps every char TS keeps and the leak detector can run both of its paths on it
/// (ADR-R3-2: deleting a hardened-only invisible could glue a neighbour onto a leak).
pub fn canonical_for_tokens(s: &str) -> String {
    let t: String = strip_invisibles_with(s, NormMode::TsExact).nfkc().collect();
    strip_invisibles_with(&t, NormMode::TsExact)
}

fn is_full(r: &Regex, s: Option<&str>) -> bool {
    s.is_some_and(|s| r.is_match(s))
}

fn num(s: &str) -> u32 {
    s.parse().expect("checked by the pattern")
}

fn parse_inner(inner: &str) -> Result<TokenBody, String> {
    let parts: Vec<&str> = inner.split(':').collect();
    let kind = parts[0];
    let part = |i: usize| parts.get(i).copied().filter(|p| !p.is_empty());
    match kind {
        "move" | "best" => {
            let (game_id, ply) = (part(1), part(2));
            if parts.len() != 3 || !is_full(&ID_RE, game_id) || !is_full(&PLY_RE, ply) {
                return Err(format!("{kind} token needs {{{{{kind}:<gameId>:<ply>}}}}"));
            }
            let (game_id, ply) = (
                game_id.unwrap_or_default().to_owned(),
                num(ply.unwrap_or("0")),
            );
            Ok(if kind == "move" {
                TokenBody::Move { game_id, ply }
            } else {
                TokenBody::Best { game_id, ply }
            })
        }
        "pv" => {
            let side = match part(3) {
                Some("best") => Some(PvSide::Best),
                Some("refutation") => Some(PvSide::Refutation),
                _ => None,
            };
            match (side, part(1), part(2), part(4)) {
                (Some(side), Some(g), Some(p), Some(n))
                    if parts.len() == 5
                        && ID_RE.is_match(g)
                        && PLY_RE.is_match(p)
                        && N_RE.is_match(n) =>
                {
                    Ok(TokenBody::Pv {
                        game_id: g.to_owned(),
                        ply: num(p),
                        side,
                        n: num(n),
                    })
                }
                _ => Err("pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}".to_owned()),
            }
        }
        "line" => {
            if parts.len() < 2 {
                return Err("line token needs {{line:<lineKey>[:<n>]}}".to_owned());
            }
            let mut key_parts = &parts[1..];
            let mut n = None;
            if let [rest @ .., last] = key_parts
                && !rest.is_empty()
                && N_RE.is_match(last)
            {
                n = Some(num(last));
                key_parts = rest;
            }
            let line_key = key_parts.join(":");
            if line_key.is_empty() || !LINE_RE.is_match(&line_key) {
                return Err("line token has an invalid line key".to_owned());
            }
            Ok(TokenBody::Line { line_key, n })
        }
        "num" => {
            let rest = inner.get(4..).unwrap_or("");
            let mut it = rest.split('|');
            let path = it.next().unwrap_or("");
            let fmt = it.next();
            if path.is_empty() || !PATH_RE.is_match(path) || it.next().is_some() {
                return Err("num token needs {{num:<path>[|<fmt>]}}".to_owned());
            }
            let fmt = match fmt {
                None => None,
                Some(f) => Some(
                    NumFormat::parse(f).ok_or_else(|| format!("unknown number format \"{f}\""))?,
                ),
            };
            Ok(TokenBody::Num {
                path: path.to_owned(),
                fmt,
            })
        }
        "date" => {
            let path = inner.get(5..).unwrap_or("");
            if path.is_empty() || !PATH_RE.is_match(path) {
                return Err("date token needs {{date:<path>}}".to_owned());
            }
            Ok(TokenBody::Date {
                path: path.to_owned(),
            })
        }
        "sq" => {
            let sq = part(1);
            if parts.len() != 2 || !is_full(&SQUARE_RE, sq) {
                return Err("sq token needs a square name a1-h8".to_owned());
            }
            Ok(TokenBody::Sq {
                square: sq.unwrap_or_default().to_owned(),
            })
        }
        "eng" => {
            let bad = || "eng token needs {{eng:<evalId>:tried|<line 1-3>[:<n>]}}".to_owned();
            let eval_id = part(1)
                .filter(|id| EVAL_ID_RE.is_match(id))
                .ok_or_else(bad)?;
            let eval_id = eval_id.to_owned();
            match parts.as_slice() {
                [_, _, "tried"] => Ok(TokenBody::Eng {
                    eval_id,
                    line: None,
                    n: None,
                }),
                [_, _, line] if ENG_LINE_RE.is_match(line) => Ok(TokenBody::Eng {
                    eval_id,
                    line: Some(num(line)),
                    n: None,
                }),
                [_, _, line, n] if ENG_LINE_RE.is_match(line) && N_RE.is_match(n) => {
                    Ok(TokenBody::Eng {
                        eval_id,
                        line: Some(num(line)),
                        n: Some(num(n)),
                    })
                }
                _ => Err(bad()),
            }
        }
        "offtopic" => {
            if parts.len() != 1 {
                return Err("offtopic token takes no arguments: {{offtopic}}".to_owned());
            }
            Ok(TokenBody::Offtopic)
        }
        _ => Err(format!("unknown token kind \"{kind}\"")),
    }
}

/// The result of [`parse_tokens`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTokens {
    pub tokens: Vec<Token>,
    /// The canonical text with every `{{…}}` replaced by [`TOKEN_PLACEHOLDER`].
    pub stripped: String,
    pub errors: Vec<TokenSyntaxError>,
}

/// Parses every `{{…}}` token of the canonical text. Each one, valid or not, becomes
/// [`TOKEN_PLACEHOLDER`] in `stripped`; malformed tokens and any leftover brace are errors.
pub fn parse_tokens(text: &str) -> ParsedTokens {
    let src = canonical_for_tokens(text);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut stripped = String::with_capacity(src.len());
    let mut last = 0;
    let hits = scan(
        &TOKEN_RE,
        &src,
        |_| true,
        |c| {
            let m = c.get(0)?;
            Some((m.end(), (m.start(), m.end())))
        },
    );
    for (start, end) in hits {
        let raw = &src[start..end];
        match parse_inner(&raw[2..raw.len() - 2]) {
            Ok(body) => tokens.push(Token {
                body,
                raw: raw.to_owned(),
                index: start,
            }),
            Err(message) => errors.push(TokenSyntaxError {
                message,
                raw: raw.to_owned(),
                index: start,
            }),
        }
        stripped.push_str(&src[last..start]);
        stripped.push(TOKEN_PLACEHOLDER);
        last = end;
    }
    stripped.push_str(&src[last..]);
    let strays = scan(
        &BRACES_RE,
        &stripped,
        |_| true,
        |c| {
            let m = c.get(0)?;
            Some((m.end(), (m.start(), m.as_str().to_owned())))
        },
    );
    errors.extend(strays.into_iter().map(|(index, raw)| TokenSyntaxError {
        message: STRAY_BRACE.to_owned(),
        raw,
        index,
    }));
    ParsedTokens {
        tokens,
        stripped,
        errors,
    }
}
