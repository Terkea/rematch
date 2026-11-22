//! Bottom-k reference-player selection (R2 Task 21, R-10, ADR-R2-11 Part 1).
//!
//! The band reference is built from **players**, not games: per cell (`band_of(rating at the
//! game)`, time class) we keep the `players_per_cell` players with the smallest
//! `h = sha256(salt ‖ lower(name))` that have enough games, and `games_per_player` games of
//! each. Both colours count: each side of a kept game is offered to its own cell.
//!
//! One pass, bounded memory. Per cell a max-heap holds at most `players_per_cell × oversample`
//! players with the smallest hashes; a player evicted from it never re-enters, because its hash
//! is fixed and the heap's maximum only falls. Each held player buffers at most
//! `games_per_player` games: the ones with the smallest `sha256(salt ‖ "game:" ‖ id)`, so the
//! games kept per player spread over the whole month instead of its first days. At the end each
//! cell takes, by ascending hash, the first `players_per_cell` players with a full buffer and at
//! least `min_games_in_time_class` games in that time class within the month (the
//! result-independent rule against provisional or barely active accounts, see
//! [`SelectOptions::min_games_in_time_class`]).
//!
//! Everything that decides the selection runs in [`GameSink::merge`], in source order, so the
//! selection is the same at any thread count, and it depends only on the salt and the dump: the
//! hash is independent of when a game was played, and the whole month is scanned, so the sample
//! is unbiased across the month. [`SelectOptions::player_rate`] admits only players whose hash
//! falls below a threshold; the bottom-k players are below it whenever a cell has enough of them,
//! so it bounds the work without changing the selection.
//!
//! Audit and privacy: the salt is generated per build (or given with `--salt` to reproduce a
//! stored build) and recorded in the build's `params.salt`, so a build can be audited and its
//! selection reproduced from the same dump. Names are never stored: PGN `White`/`Black` tags are
//! redacted to `?` and headers are blanked in [`SelectedGame`]. With the stored salt, a hash can
//! be matched to a name by hashing candidate usernames, so the hashes are pseudonyms for
//! grouping a player's games, not a secret (ADR-R2-11).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chess_core::TimeClass;
use futures::future::BoxFuture;
use profile_core::bands::band_of;
use profile_core::types::Band;
use queue::queue::Queue;
use queue::{
    EnqueueOptions, KIND_REFERENCE_GAME, REFERENCE_PRIORITY, ReferenceGamePayload, refgame_key,
};
use refdata::builds::{BuildStatus, get_build};
use refdata::reference_games::{NewReferenceGame, insert_reference_games};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::download::DumpError;
use crate::headers::DumpHeader;
use crate::pipeline::GameSink;
use crate::sample::SampleFilter;

/// The reference's time classes (as the explorer's; correspondence is excluded).
pub const REFERENCE_TIME_CLASSES: [TimeClass; 4] = [
    TimeClass::Bullet,
    TimeClass::Blitz,
    TimeClass::Rapid,
    TimeClass::Classical,
];

/// Selection sizes (R-10).
#[derive(Clone, Debug, PartialEq)]
pub struct SelectOptions {
    /// Players selected per cell (20).
    pub players_per_cell: usize,
    /// Games per selected player (10).
    pub games_per_player: usize,
    /// Heap size factor: a cell holds `players_per_cell × oversample` candidates (20).
    pub oversample: usize,
    /// Target of games admitted into the selection (3 000 000). The build sets
    /// [`player_rate`](Self::player_rate) so a month of the expected size admits about this
    /// many. Admission stops at [`admission_cap`] (1.5x), which marks the build `capped`; a
    /// capped reference build enqueues nothing and fails, because stopping admission is a
    /// first-N cut of the rest of the month.
    pub max_scan_games: u64,
    /// A selected player has at least this many games in the cell's time class within the dump
    /// month (20). The dump has no provisional flag; Lichess ratings stop being provisional as
    /// the rating deviation falls with games played, so a player active in that time class is
    /// unlikely to be provisional. The rule counts games, never results or rating changes, so it
    /// does not bias the reference. The value 20 is an **unverified estimate**, not checked
    /// against Lichess data.
    pub min_games_in_time_class: usize,
    /// Only players with `hash_fraction(h) < player_rate` are admitted (1.0 = all). The build
    /// derives it from the expected month size with [`player_rate`].
    pub player_rate: f64,
}

impl Default for SelectOptions {
    fn default() -> Self {
        Self {
            players_per_cell: 20,
            games_per_player: 10,
            oversample: 20,
            max_scan_games: 3_000_000,
            min_games_in_time_class: 20,
            player_rate: 1.0,
        }
    }
}

/// A 32-byte salt from 64 hex digits (the `--salt` override, `params.salt`).
pub fn parse_salt(s: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(s.trim()).map_err(|e| format!("salt is not hex: {e}"))?;
    bytes
        .try_into()
        .map_err(|b: Vec<u8>| format!("salt must be 32 bytes (64 hex digits), got {}", b.len()))
}

/// `sha256(salt ‖ lower(name))`.
pub fn player_hash(salt: &[u8; 32], name: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(name.to_lowercase().as_bytes());
    h.finalize().into()
}

fn game_key(salt: &[u8; 32], lichess_game_id: &str) -> u64 {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(b"game:");
    h.update(lichess_game_id.as_bytes());
    let d: [u8; 32] = h.finalize().into();
    u64::from_be_bytes(d[..8].try_into().expect("8 bytes"))
}

/// The first 8 bytes of a hash as a fraction in `[0, 1)`.
pub fn hash_fraction(h: &[u8; 32]) -> f64 {
    let x = u64::from_be_bytes(h[..8].try_into().expect("8 bytes"));
    x as f64 / 18_446_744_073_709_551_616.0
}

/// The player rate that admits about `max_scan_games` of `expected` games: each admitted player
/// brings all of their games and a game passes when either side does, so
/// `min(1, max_scan_games / (2 × expected))`.
pub fn player_rate(max_scan_games: u64, expected: u64) -> f64 {
    if expected == 0 {
        return 1.0;
    }
    (max_scan_games as f64 / (2.0 * expected as f64)).min(1.0)
}

/// The admission cap for a target of `max_scan_games`: a 1.5x margin (T21 review I3), so a
/// month up to half again as large as estimated (a 31-day month after a 28-day one, growth, the
/// 100M default) still reads to the end.
pub fn admission_cap(max_scan_games: u64) -> u64 {
    max_scan_games.saturating_add(max_scan_games / 2)
}

/// The buffer bound: cells × candidates × games × `avg_pgn_bytes` (a game held for both colours
/// is shared, so this is an upper bound).
pub fn memory_bound_bytes(o: &SelectOptions, avg_pgn_bytes: u64) -> u64 {
    let cells = (Band::ALL.len() * REFERENCE_TIME_CLASSES.len()) as u64;
    cells * (o.players_per_cell * o.oversample) as u64 * o.games_per_player as u64 * avg_pgn_bytes
}

/// The pipeline filter of a reference pass: every eligible game of the month (no game-level
/// sampling, which would thin each player's games), rated with a real clock and both ratings,
/// no BOTs, the four time classes, clocks required (the reference metrics use them). The
/// `RatingDiff` provisional heuristic stays off: it filters by result.
pub fn reference_filter() -> SampleFilter {
    SampleFilter {
        max_games: None,
        sample_rate: 1.0,
        seed: 0,
        require_clock: true,
        time_classes: Some(REFERENCE_TIME_CLASSES.to_vec()),
        exclude_bots: true,
        exclude_provisional: false,
    }
}

/// `pgn` with the `White` and `Black` tag values replaced by `?`.
pub fn redact_pgn(pgn: &str) -> String {
    let mut out = String::with_capacity(pgn.len());
    for line in pgn.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let eol = &line[body.len()..];
        if body.starts_with("[White \"") {
            out.push_str("[White \"?\"]");
            out.push_str(eol);
        } else if body.starts_with("[Black \"") {
            out.push_str("[Black \"?\"]");
            out.push_str(eol);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// How full one cell ended (T21 review I2): every band x time class is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellFill {
    pub band: Band,
    pub tc: TimeClass,
    /// `players_per_cell`.
    pub target: usize,
    /// Players held in the cell's heap at the end.
    pub candidates: usize,
    /// Candidates with `games_per_player` buffered games.
    pub full: usize,
    /// Full candidates that also pass `min_games_in_time_class`.
    pub eligible: usize,
    /// Players selected (`min(eligible, target)`).
    pub chosen: usize,
}

impl CellFill {
    /// Fewer players than the target.
    pub fn under_filled(&self) -> bool {
        self.chosen < self.target
    }
}

/// One selected (player, game). A game selected for both colours appears twice, once per
/// player; [`enqueue_reference_games`] inserts it once.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedGame {
    /// The selected player's hash, hex.
    pub player_hash: String,
    pub band: Band,
    pub tc: TimeClass,
    /// The game's PGN with both names redacted.
    pub pgn: String,
    /// The dump header with `white`/`black` set to `?`.
    pub header: DumpHeader,
    pub white_hash: String,
    pub black_hash: String,
}

#[derive(Debug)]
struct GameRef {
    key: u64,
    pgn: String,
    header: DumpHeader,
    tc: TimeClass,
    white_hash: String,
    black_hash: String,
}

/// A buffered game, ordered by its key (then id).
#[derive(Clone, Debug)]
struct Held(Arc<GameRef>);

impl Held {
    fn order(&self) -> (u64, &str) {
        (self.0.key, self.0.header.lichess_game_id.as_str())
    }
}

impl PartialEq for Held {
    fn eq(&self, other: &Self) -> bool {
        self.order() == other.order()
    }
}
impl Eq for Held {}
impl PartialOrd for Held {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Held {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order().cmp(&other.order())
    }
}

/// What `observe` hands to `merge`: an admitted game and the sides that passed the rate.
pub struct Admitted {
    game: Arc<GameRef>,
    /// `(hash, band)` per admitted side.
    sides: Vec<([u8; 32], Band)>,
}

#[derive(Default)]
struct Cell {
    /// Max-heap of the held players' hashes.
    heap: BinaryHeap<[u8; 32]>,
    /// Each held player's games (a max-heap by key, at most `games_per_player`).
    players: HashMap<[u8; 32], BinaryHeap<Held>>,
}

/// Counters of a [`ReferenceSink`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferenceSinkStats {
    /// Games with at least one side under the player rate, before the cap.
    pub games_offered: u64,
    /// Games admitted (at most `max_scan_games`).
    pub games_admitted: u64,
    /// Sides offered to a full cell with a larger hash than its maximum.
    pub sides_rejected: u64,
    /// Players evicted from a cell's heap.
    pub evictions: u64,
    /// `max_scan_games` stopped admission before the end of the dump.
    pub capped: bool,
}

#[derive(Default)]
struct State {
    cells: HashMap<(Band, TimeClass), Cell>,
    /// Admitted games per (player, time class), across bands.
    tc_games: HashMap<([u8; 32], TimeClass), u32>,
    stats: ReferenceSinkStats,
}

/// The [`GameSink`] of a reference pass.
pub struct ReferenceSink {
    salt: [u8; 32],
    opts: SelectOptions,
    state: Mutex<State>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ReferenceSink {
    /// Logs the buffer bound (at 2 KB per PGN).
    pub fn new(salt: [u8; 32], opts: SelectOptions) -> Self {
        tracing::info!(
            players_per_cell = opts.players_per_cell,
            games_per_player = opts.games_per_player,
            oversample = opts.oversample,
            max_scan_games = opts.max_scan_games,
            player_rate = opts.player_rate,
            memory_bound_mb = memory_bound_bytes(&opts, 2_048) / 1_000_000,
            "reference selection: buffers at most cells x players x games x PGN size"
        );
        Self {
            salt,
            opts,
            state: Mutex::default(),
        }
    }

    pub fn stats(&self) -> ReferenceSinkStats {
        lock(&self.state).stats
    }

    fn cap(&self) -> usize {
        (self.opts.players_per_cell * self.opts.oversample).max(1)
    }

    /// The selection, ordered by cell (band, time class), player hash and game key.
    pub fn finish(self) -> Vec<SelectedGame> {
        self.finish_with_fill().0
    }

    /// The selection and the fill of every cell (band x time class, in that order).
    pub fn finish_with_fill(self) -> (Vec<SelectedGame>, Vec<CellFill>) {
        let state = self
            .state
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        let tc_index = |tc: TimeClass| {
            REFERENCE_TIME_CLASSES
                .iter()
                .position(|t| *t == tc)
                .unwrap_or(usize::MAX)
        };
        let mut cells = state.cells;
        for band in Band::ALL {
            for tc in REFERENCE_TIME_CLASSES {
                cells.entry((band, tc)).or_default();
            }
        }
        let mut cells: Vec<_> = cells.into_iter().collect();
        cells.sort_by_key(|((band, tc), _)| (*band, tc_index(*tc)));
        let mut out = Vec::new();
        let mut fill = Vec::with_capacity(cells.len());
        for ((band, tc), cell) in cells {
            let mut players: Vec<_> = cell.players.into_iter().collect();
            players.sort_by_key(|p| p.0);
            let candidates = players.len();
            let full: Vec<_> = players
                .into_iter()
                .filter(|(_, games)| games.len() >= self.opts.games_per_player)
                .collect();
            let n_full = full.len();
            let eligible: Vec<_> = full
                .into_iter()
                .filter(|(h, _)| {
                    state.tc_games.get(&(*h, tc)).copied().unwrap_or(0) as usize
                        >= self.opts.min_games_in_time_class
                })
                .collect();
            let n_eligible = eligible.len();
            let chosen: Vec<_> = eligible
                .into_iter()
                .take(self.opts.players_per_cell)
                .collect();
            fill.push(CellFill {
                band,
                tc,
                target: self.opts.players_per_cell,
                candidates,
                full: n_full,
                eligible: n_eligible,
                chosen: chosen.len(),
            });
            for (h, games) in chosen {
                let player_hash = hex::encode(h);
                for Held(g) in games.into_sorted_vec() {
                    let mut header = g.header.clone();
                    header.white = "?".into();
                    header.black = "?".into();
                    out.push(SelectedGame {
                        player_hash: player_hash.clone(),
                        band,
                        tc,
                        pgn: g.pgn.clone(),
                        header,
                        white_hash: g.white_hash.clone(),
                        black_hash: g.black_hash.clone(),
                    });
                }
            }
        }
        (out, fill)
    }

    fn offer(&self, st: &mut State, h: [u8; 32], band: Band, game: &Arc<GameRef>) {
        let cap = self.cap();
        let gpp = self.opts.games_per_player.max(1);
        let cell = st.cells.entry((band, game.tc)).or_default();
        if !cell.players.contains_key(&h) {
            if cell.heap.len() >= cap {
                let max = *cell.heap.peek().expect("a full heap");
                if h >= max {
                    st.stats.sides_rejected += 1;
                    return;
                }
                cell.heap.pop();
                cell.players.remove(&max);
                st.stats.evictions += 1;
            }
            cell.heap.push(h);
            cell.players.insert(h, BinaryHeap::new());
        }
        let games = cell.players.get_mut(&h).expect("held");
        games.push(Held(Arc::clone(game)));
        if games.len() > gpp {
            games.pop();
        }
    }
}

impl GameSink for ReferenceSink {
    type Partial = Option<Admitted>;

    fn observe(&self, pgn: &str, h: &DumpHeader, tc: TimeClass) -> Self::Partial {
        let (we, be) = (h.white_elo?, h.black_elo?);
        let wh = player_hash(&self.salt, &h.white);
        let bh = player_hash(&self.salt, &h.black);
        let rate = self.opts.player_rate;
        let mut sides = Vec::with_capacity(2);
        if hash_fraction(&wh) < rate {
            sides.push((wh, band_of(f64::from(we))));
        }
        if hash_fraction(&bh) < rate {
            sides.push((bh, band_of(f64::from(be))));
        }
        if sides.is_empty() {
            return None;
        }
        Some(Admitted {
            game: Arc::new(GameRef {
                key: game_key(&self.salt, &h.lichess_game_id),
                pgn: redact_pgn(pgn),
                header: h.clone(),
                tc,
                white_hash: hex::encode(wh),
                black_hash: hex::encode(bh),
            }),
            sides,
        })
    }

    fn merge(&self, parts: Vec<Self::Partial>) -> BoxFuture<'_, Result<(), DumpError>> {
        Box::pin(async move {
            let mut st = lock(&self.state);
            for a in parts.into_iter().flatten() {
                st.stats.games_offered += 1;
                if st.stats.games_admitted >= admission_cap(self.opts.max_scan_games) {
                    st.stats.capped = true;
                    continue;
                }
                st.stats.games_admitted += 1;
                for (h, band) in &a.sides {
                    *st.tc_games.entry((*h, a.game.tc)).or_default() += 1;
                    self.offer(&mut st, *h, *band, &a.game);
                }
            }
            Ok(())
        })
    }
}

fn store(e: impl std::fmt::Display) -> DumpError {
    DumpError::Store(e.to_string())
}

/// Inserts the selected games of a verified `building` build as `pending` reference games (a
/// game selected for both colours once) and enqueues one `analyse-reference-game` job per game
/// at priority 200 with dedupe key `refgame:<id>`. Refuses a build without a source SHA, so
/// nothing is inserted before the dump is verified (R-9). Idempotent: a retry finds the same
/// rows and the live jobs. Returns the number of distinct games.
pub async fn enqueue_reference_games(
    pool: &PgPool,
    queue: &Queue,
    build_id: Uuid,
    games: &[SelectedGame],
) -> Result<usize, DumpError> {
    let build = get_build(pool, build_id)
        .await
        .map_err(store)?
        .ok_or_else(|| DumpError::Store(format!("build {build_id} not found")))?;
    if build.source_sha256.is_empty() {
        return Err(DumpError::Store(format!(
            "build {build_id} has no verified source SHA-256; reference games wait for verification"
        )));
    }
    // The sampled sides of each game, over every (player, game) entry.
    let mut sides: HashMap<&str, (bool, bool)> = HashMap::new();
    for g in games {
        let e = sides.entry(g.header.lichess_game_id.as_str()).or_default();
        e.0 |= g.player_hash == g.white_hash;
        e.1 |= g.player_hash == g.black_hash;
    }
    let mut seen = HashSet::new();
    let rows: Vec<NewReferenceGame> = games
        .iter()
        .filter(|g| seen.insert(g.header.lichess_game_id.as_str()))
        .map(|g| {
            let (white_selected, black_selected) = sides[g.header.lichess_game_id.as_str()];
            NewReferenceGame {
                lichess_game_id: g.header.lichess_game_id.clone(),
                pgn: g.pgn.clone(),
                time_class: g.tc,
                base_seconds: g.header.base_seconds.and_then(|s| i32::try_from(s).ok()),
                increment_seconds: g
                    .header
                    .increment_seconds
                    .and_then(|s| i32::try_from(s).ok()),
                white_rating: g.header.white_elo,
                black_rating: g.header.black_elo,
                white_hash: g.white_hash.clone(),
                black_hash: g.black_hash.clone(),
                white_selected,
                black_selected,
                result: g.header.result.clone(),
                termination: g.header.termination.clone(),
                played_at: g.header.utc,
            }
        })
        .collect();
    let ids = insert_reference_games(pool, build_id, &rows)
        .await
        .map_err(store)?;
    resume_reference_enqueue(pool, queue, build_id).await?;
    Ok(ids.len())
}

/// Enqueues an `analyse-reference-game` job for every `pending` reference game of a verified
/// `building` build (T21 review I4). Idempotent: the `refgame:<id>` dedupe key skips games whose
/// job is still live, so this resumes an enqueue that crashed between the row insert and the
/// last job batch. Task 35 schedules it for `building` builds. Returns the jobs added.
pub async fn resume_reference_enqueue(
    pool: &PgPool,
    queue: &Queue,
    build_id: Uuid,
) -> Result<usize, DumpError> {
    let build = get_build(pool, build_id)
        .await
        .map_err(store)?
        .ok_or_else(|| DumpError::Store(format!("build {build_id} not found")))?;
    if build.status != BuildStatus::Building || build.source_sha256.is_empty() {
        return Err(DumpError::Store(format!(
            "build {build_id} is {} with source SHA {:?}: only a verified building build takes \
             reference jobs",
            build.status, build.source_sha256
        )));
    }
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM reference_games WHERE build_id = $1 AND status = 'pending' ORDER BY id",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await
    .map_err(store)?;
    let jobs: Vec<_> = ids
        .iter()
        .map(|id| {
            let payload = serde_json::to_value(ReferenceGamePayload {
                reference_game_id: *id,
            })
            .expect("a payload serialises");
            let opts = EnqueueOptions {
                dedupe_key: Some(refgame_key(*id)),
                priority: REFERENCE_PRIORITY,
                ..EnqueueOptions::default()
            };
            (payload, opts)
        })
        .collect();
    let mut added = 0u64;
    for chunk in jobs.chunks(1_000) {
        added += queue
            .enqueue_many(KIND_REFERENCE_GAME, chunk)
            .await
            .map_err(store)?;
    }
    Ok(usize::try_from(added).unwrap_or(usize::MAX))
}
