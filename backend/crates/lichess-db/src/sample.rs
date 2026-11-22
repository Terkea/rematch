//! Game sampling (R2 Task 19).
//!
//! A game is kept when it is rated play with a real clock and both ratings: `TimeControl "-"`
//! (correspondence), `Termination` `Abandoned` / `Rules infraction` and a missing Elo are
//! skipped. Sampling is deterministic: a game is kept when
//! `fnv1a32(site ‖ decimal(seed)) / 2^32 < sample_rate`, so the same seed keeps the same games
//! on every run and every machine.

use chess_core::TimeClass;
use chess_core::time_control::classify_time_control;
use futures::stream::{Stream, StreamExt};

use crate::download::DumpError;
use crate::headers::{DumpHeader, parse_dump_headers};

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// Which games to keep.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleFilter {
    /// Stop after this many kept games (the caller then drains the stream, R-9).
    pub max_games: Option<u64>,
    /// Fraction of eligible games to keep, `1.0` = all.
    pub sample_rate: f64,
    pub seed: u32,
    /// Drop games without `[%clk` comments.
    pub require_clock: bool,
    /// Keep only these classes (`None` = all).
    pub time_classes: Option<Vec<TimeClass>>,
    /// Drop games where either side has the `BOT` title (default `true`).
    pub exclude_bots: bool,
    /// Drop games where either side looks provisional
    /// ([`crate::headers::looks_provisional`]; default `false`).
    pub exclude_provisional: bool,
}

impl Default for SampleFilter {
    fn default() -> Self {
        Self {
            max_games: None,
            sample_rate: 1.0,
            seed: 0,
            require_clock: false,
            time_classes: None,
            exclude_bots: true,
            exclude_provisional: false,
        }
    }
}

/// A kept game.
#[derive(Clone, Debug, PartialEq)]
pub struct SampledGame {
    pub pgn: String,
    pub header: DumpHeader,
    pub time_class: TimeClass,
}

fn fnv1a32_update(mut hash: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// 32-bit FNV-1a.
pub fn fnv1a32(bytes: &[u8]) -> u32 {
    fnv1a32_update(FNV_OFFSET_BASIS, bytes)
}

/// `fnv1a32(site ‖ decimal(seed)) / 2^32`, in `[0, 1)`.
pub fn sample_fraction(site: &str, seed: u32) -> f64 {
    let mut digits = [0u8; 10];
    let mut n = seed;
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let hash = fnv1a32_update(fnv1a32(site.as_bytes()), &digits[start..]);
    f64::from(hash) / 4_294_967_296.0
}

/// The sample rate that keeps about `target` of `expected` games, spread over the whole dump:
/// `min(1, target / expected)`, or `1` when nothing is expected. The dump is in time order, so
/// stopping after the first `target` games would sample only the first hours of the month; with
/// this rate `max_games` is only a safety cap.
pub fn proportional_rate(target: u64, expected: u64) -> f64 {
    if expected == 0 {
        return 1.0;
    }
    (target as f64 / expected as f64).min(1.0)
}

/// The game's time class when `filter` keeps it, else `None`.
pub fn keep(h: &DumpHeader, f: &SampleFilter) -> Option<TimeClass> {
    if h.time_control == "-" {
        return None;
    }
    let (base, inc) = (h.base_seconds?, h.increment_seconds?);
    if matches!(
        h.termination.as_deref(),
        Some("Abandoned" | "Rules infraction")
    ) {
        return None;
    }
    h.white_elo?;
    h.black_elo?;
    if f.exclude_bots && h.has_bot() {
        return None;
    }
    if f.exclude_provisional && (h.white_provisional() || h.black_provisional()) {
        return None;
    }
    if f.require_clock && !h.has_clock {
        return None;
    }
    let tc = classify_time_control(base, inc);
    if f.time_classes
        .as_ref()
        .is_some_and(|list| !list.contains(&tc))
    {
        return None;
    }
    (sample_fraction(&h.site, f.seed) < f.sample_rate).then_some(tc)
}

/// Counters of a [`Sampler`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleStats {
    /// Games offered while the sampler was not full.
    pub seen: u64,
    pub kept: u64,
    /// Games whose headers did not parse (skipped, counted in `seen`).
    pub malformed: u64,
}

/// Applies a [`SampleFilter`] game by game and counts.
#[derive(Clone, Debug)]
pub struct Sampler {
    filter: SampleFilter,
    stats: SampleStats,
}

impl Sampler {
    pub fn new(filter: SampleFilter) -> Self {
        Self {
            filter,
            stats: SampleStats::default(),
        }
    }

    pub fn filter(&self) -> &SampleFilter {
        &self.filter
    }

    pub fn stats(&self) -> SampleStats {
        self.stats
    }

    /// True once `max_games` games were kept.
    pub fn is_full(&self) -> bool {
        self.filter
            .max_games
            .is_some_and(|max| self.stats.kept >= max)
    }

    /// Parses and filters one game. A game with malformed headers is skipped and counted.
    /// Returns `None` without counting when the sampler is already full.
    pub fn offer(&mut self, pgn: String) -> Option<SampledGame> {
        if self.is_full() {
            return None;
        }
        self.stats.seen += 1;
        let header = match parse_dump_headers(&pgn) {
            Ok(h) => h,
            Err(err) => {
                self.stats.malformed += 1;
                tracing::debug!(error = %err, "skipping a game with malformed headers");
                return None;
            }
        };
        let time_class = keep(&header, &self.filter)?;
        self.stats.kept += 1;
        Some(SampledGame {
            pgn,
            header,
            time_class,
        })
    }
}

/// The kept games of `games`. Borrows the source, so after `max_games` the caller still owns
/// the rest of it (to [`crate::download::drain_to_end`] a dump stream). Stops pulling as soon
/// as the sampler is full; a source error is passed through.
pub fn filter_sample<'a, S>(
    games: &'a mut S,
    filter: SampleFilter,
) -> impl Stream<Item = Result<SampledGame, DumpError>> + 'a
where
    S: Stream<Item = Result<String, DumpError>> + Unpin + ?Sized,
{
    futures::stream::unfold(
        (games, Sampler::new(filter)),
        |(games, mut sampler)| async move {
            loop {
                if sampler.is_full() {
                    return None;
                }
                match games.next().await? {
                    Ok(pgn) => {
                        if let Some(game) = sampler.offer(pgn) {
                            return Some((Ok(game), (games, sampler)));
                        }
                    }
                    Err(err) => return Some((Err(err), (games, sampler))),
                }
            }
        },
    )
}
