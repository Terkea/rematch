//! Lichess importer (port of `apps/worker/src/importers/lichess.ts`).

use std::sync::Arc;

use chess_core::time_control::classify_time_control;
use chess_core::{EngineScore, TimeClass};
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::Value;
use time::OffsetDateTime;

use crate::types::{
    FetchOptions, GameSource, Headers, NormalizedGame, Platform, PlatformClient, PlatformError,
    Player,
};

pub struct LichessSource {
    pub token: Option<String>,
}

fn epoch_ms(t: OffsetDateTime) -> i128 {
    t.unix_timestamp_nanos().div_euclid(1_000_000)
}

/// `encodeURIComponent`: everything but `A-Za-z0-9 - _ . ! ~ * ' ( )` is percent-encoded.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Parameter order matches the TS `URLSearchParams` sequence.
pub fn lichess_games_url(username: &str, opts: &FetchOptions) -> String {
    let mut params = Vec::new();
    if let Some(t) = opts.since {
        params.push(format!("since={}", epoch_ms(t)));
    }
    if let Some(t) = opts.until {
        params.push(format!("until={}", epoch_ms(t)));
    }
    if let Some(m) = opts.max {
        params.push(format!("max={m}"));
    }
    params.extend(
        [
            "sort=dateDesc",
            "pgnInJson=true",
            "clocks=true",
            "evals=true",
            "opening=true",
        ]
        .map(String::from),
    );
    format!(
        "https://lichess.org/api/games/user/{}?{}",
        encode_uri_component(&username.to_lowercase()),
        params.join("&")
    )
}

fn to_result(g: &Value) -> &'static str {
    match g["winner"].as_str() {
        Some("white") => return "1-0",
        Some("black") => return "0-1",
        _ => {}
    }
    // outoftime without a winner means timeout vs insufficient material: a draw.
    match g["status"].as_str() {
        Some("draw" | "stalemate" | "outoftime") => "1/2-1/2",
        _ => "*",
    }
}

fn player_of(p: &Value) -> Player {
    let name = if let Some(n) = p["user"]["name"].as_str() {
        n.to_owned()
    } else if let Some(l) = p["aiLevel"].as_i64() {
        format!("Stockfish level {l}")
    } else {
        "Anonymous".to_owned()
    };
    Player {
        name,
        rating: p["rating"].as_i64().map(|r| r as i32),
    }
}

fn to_evals(g: &Value) -> Option<Vec<Option<EngineScore>>> {
    let analysis = g["analysis"].as_array()?;
    let ply_count = match g["moves"].as_str() {
        Some(m) if !m.is_empty() => m.split(' ').filter(|s| !s.is_empty()).count(),
        _ => analysis.len(),
    };
    Some(
        (0..ply_count)
            .map(|i| {
                let a = analysis.get(i)?;
                if let Some(m) = a["mate"].as_i64() {
                    Some(EngineScore::Mate(m as i32))
                } else {
                    a["eval"].as_i64().map(|c| EngineScore::Cp(c as i32))
                }
            })
            .collect(),
    )
}

fn timing(g: &Value) -> (TimeClass, Option<i32>, Option<i32>) {
    let clock = &g["clock"];
    match (clock["initial"].as_i64(), clock["increment"].as_i64()) {
        (Some(initial), Some(inc)) if g["speed"].as_str() != Some("correspondence") => (
            classify_time_control(initial.max(0) as u32, inc.max(0) as u32),
            Some(initial as i32),
            Some(inc as i32),
        ),
        _ => (TimeClass::Daily, None, None),
    }
}

/// Returns `None` for games the importer skips (variants, aborted/noStart, no pgn).
pub fn normalize_lichess(g: &Value) -> Option<NormalizedGame> {
    if g["variant"].as_str().is_some_and(|v| v != "standard") {
        return None;
    }
    if matches!(g["status"].as_str(), Some("aborted" | "noStart")) {
        return None;
    }
    let pgn = g["pgn"].as_str().filter(|p| !p.is_empty())?;
    let Some(id) = g["id"].as_str() else {
        tracing::warn!("skipping lichess game without an id");
        return None;
    };
    let Some(played_at) = g["createdAt"]
        .as_i64()
        .and_then(|c| OffsetDateTime::from_unix_timestamp_nanos(i128::from(c) * 1_000_000).ok())
    else {
        tracing::warn!(
            game_id = id,
            "skipping lichess game without a valid createdAt"
        );
        return None;
    };
    let (time_class, base_seconds, increment_seconds) = timing(g);
    Some(NormalizedGame {
        platform: Platform::Lichess,
        platform_game_id: id.to_owned(),
        pgn: pgn.to_owned(),
        played_at,
        time_class,
        base_seconds,
        increment_seconds,
        rated: g["rated"].as_bool().unwrap_or(false),
        white: player_of(&g["players"]["white"]),
        black: player_of(&g["players"]["black"]),
        result: to_result(g).to_owned(),
        termination: g["status"].as_str().map(str::to_owned),
        eco: g["opening"]["eco"].as_str().map(str::to_owned),
        opening_name: g["opening"]["name"].as_str().map(str::to_owned),
        lichess_evals: to_evals(g),
    })
}

impl GameSource for LichessSource {
    fn fetch<'a>(
        &'a self,
        client: Arc<dyn PlatformClient>,
        username: &'a str,
        opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        let url = lichess_games_url(username, &opts);
        let mut headers: Headers = vec![("Accept".into(), "application/x-ndjson".into())];
        if let Some(t) = self.token.as_deref().filter(|t| !t.is_empty()) {
            headers.push(("Authorization".into(), format!("Bearer {t}")));
        }
        owned_stream(client, url, headers)
    }
}

/// `get_ndjson` borrows the client and URL, so a stream that owns them runs the borrowing stream
/// in a task and forwards items over a bounded channel. Dropping the stream aborts the task,
/// which releases the client's request lock.
fn owned_stream<'a>(
    client: Arc<dyn PlatformClient>,
    url: String,
    headers: Headers,
) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
    /// Aborts the task when the consumer drops the stream.
    struct Abort(Option<tokio::task::JoinHandle<()>>);
    impl Drop for Abort {
        fn drop(&mut self) {
            if let Some(h) = &self.0 {
                h.abort();
            }
        }
    }
    let (mut tx, rx) = futures::channel::mpsc::channel(1);
    let task = tokio::spawn(async move {
        let mut raw = client.get_ndjson(&url, headers);
        while let Some(item) = raw.next().await {
            let failed = item.is_err();
            let out = match item {
                Ok(v) => match normalize_lichess(&v) {
                    Some(g) => Ok(g),
                    None => continue,
                },
                Err(e) => Err(e),
            };
            if futures::SinkExt::send(&mut tx, out).await.is_err() || failed {
                break;
            }
        }
    });
    futures::stream::unfold(Some((rx, Abort(Some(task)))), |state| async move {
        let (mut rx, mut guard) = state?;
        if let Some(item) = rx.next().await {
            return Some((item, Some((rx, guard))));
        }
        // Channel closed: surface a panicked or cancelled task instead of a clean end.
        let handle = guard.0.take()?;
        match handle.await {
            Ok(()) => None,
            Err(e) => Some((
                Err(PlatformError::Transport(format!(
                    "lichess stream task failed: {e}"
                ))),
                None,
            )),
        }
    })
    .boxed()
}
