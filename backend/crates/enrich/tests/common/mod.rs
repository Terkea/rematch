//! Shared helpers for the Task 25 enrichment tests: games built from UCI moves, fake Maia and a
//! fake tablebase.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chess_core::board::Position;
use chess_core::move_record::build_move_records;
use chess_core::{Color, EngineScore, MoveRecord, ParsedGame, Ply, PositionEval, START_FEN};
use engine::Analyser;
use enrich::difficulty::{DifficultyOutcome, DifficultySource, PendingReason};
use enrich::enrich_game::{EnrichDeps, EnrichInput};
use enrich::store::{EnrichmentStore, MemoryEnrichmentStore};
use enrich::types::{DifficultyInput, TablebaseProbe};
use futures::future::BoxFuture;
use profile_core::types::{Difficulty, Wdl};
use tokio_util::sync::CancellationToken;

pub const MODEL: &str = "maia2-rapid-0.11.0";

/// Ruy Lopez, Breyer: 24 plies, no checks, so every ply has a null-move FEN.
pub const RUY: &[&str] = &[
    "e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6", "e1g1", "f8e7", "f1e1", "b7b5",
    "a4b3", "d7d6", "c2c3", "e8g8", "h2h3", "c6b8", "d2d4", "b8d7", "b1d2", "c8b7", "b3c2", "f8e8",
];

/// 1. e4 f5 2. Qh5+ g6: Black is in check before ply 4.
pub const CHECK: &[&str] = &["e2e4", "f7f5", "d1h5", "g7g6"];

/// Six pieces; 1. bxc3 (5 left) h6 2. Ra8+.
pub const ENDGAME_FEN: &str = "4k3/7p/8/8/8/2n5/1P6/R3K3 w - - 0 60";
pub const ENDGAME: &[&str] = &["b2c3", "h7h6", "a1a8"];

pub fn game_from(start: &str, ucis: &[&str]) -> ParsedGame {
    let mut pos = Position::from_fen(start).expect("start fen");
    let mut plies = Vec::new();
    for (i, uci) in ucis.iter().enumerate() {
        let fen_before = pos.fen();
        let color = pos.side_to_move();
        let played = pos.push_uci(uci).expect("legal move");
        plies.push(Ply {
            ply: i as u32 + 1,
            color,
            san: played.san,
            uci: played.uci,
            fen_before,
            fen_after: pos.fen(),
            clock_seconds: None,
        });
    }
    ParsedGame {
        headers: Vec::new(),
        start_fen: start.to_owned(),
        plies,
        terminal: None,
    }
}

pub fn game(ucis: &[&str]) -> ParsedGame {
    game_from(START_FEN, ucis)
}

/// Level evals (cp 0, white's view) with `pv` for every position (`plies + 1` entries).
pub fn evals(parsed: &ParsedGame, pv: &[&str]) -> Vec<PositionEval> {
    (0..=parsed.plies.len())
        .map(|_| PositionEval {
            score: EngineScore::Cp(0),
            best_uci: pv.first().map(|s| (*s).to_owned()),
            pv: pv.iter().map(|s| (*s).to_owned()).collect(),
        })
        .collect()
}

/// Records with ΔW set per ply (`(ply, ΔW)`), every other ply at 0.
pub fn records(parsed: &ParsedGame, evals: &[PositionEval], dw: &[(u32, f64)]) -> Vec<MoveRecord> {
    let mut recs = build_move_records(parsed, evals, None).expect("records");
    for r in &mut recs {
        r.delta_w = dw
            .iter()
            .find(|(p, _)| *p == r.ply)
            .map_or(0.0, |(_, d)| *d);
        r.win_after = r.win_before - r.delta_w;
    }
    recs
}

pub struct Game {
    pub parsed: ParsedGame,
    pub evals: Vec<PositionEval>,
    pub records: Vec<MoveRecord>,
    pub colors: Vec<Color>,
}

impl Game {
    pub fn new(parsed: ParsedGame, pv: &[&str], dw: &[(u32, f64)], colors: &[Color]) -> Self {
        let evals = evals(&parsed, pv);
        let records = records(&parsed, &evals, dw);
        Self {
            parsed,
            evals,
            records,
            colors: colors.to_vec(),
        }
    }

    pub fn input(&self) -> EnrichInput<'_> {
        EnrichInput {
            parsed: &self.parsed,
            evals: &self.evals,
            records: &self.records,
            colors: &self.colors,
            elo_white: Some(1500),
            elo_black: Some(1600),
        }
    }
}

pub fn deps(
    analyser: Arc<dyn Analyser>,
    store: Arc<dyn EnrichmentStore>,
    tb: Option<Arc<dyn TablebaseProbe>>,
    difficulty: Option<Arc<dyn DifficultySource>>,
) -> EnrichDeps {
    EnrichDeps {
        analyser,
        store,
        tb,
        difficulty,
        cancel: CancellationToken::new(),
    }
}

pub fn memory_store() -> Arc<dyn EnrichmentStore> {
    Arc::new(MemoryEnrichmentStore::default())
}

/// What the fake Maia answers while it is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaiaMode {
    /// A valid Maia estimate for every input.
    Ok,
    /// The service answers, but every item fails validation (a deterministic model error).
    Invalid,
    /// `/healthz` answers, but predict calls fail (a call-level failure).
    FailsCalls,
    /// The predict call fails and Maia is down from then on (`/healthz` fails too).
    GoesDown,
}

/// A fake Maia: down or up (`/healthz`), recording every input it is sent.
pub struct FakeMaia {
    pub up: AtomicBool,
    pub mode: Mutex<MaiaMode>,
    pub sent: Mutex<Vec<DifficultyInput>>,
    pub health_checks: Mutex<u32>,
    /// Every call that includes this FEN fails (call-level), whatever the mode.
    pub fail_fen: Mutex<Option<String>>,
}

impl FakeMaia {
    pub fn new(up: bool, mode: MaiaMode) -> Arc<Self> {
        Arc::new(Self {
            up: AtomicBool::new(up),
            mode: Mutex::new(mode),
            sent: Mutex::new(Vec::new()),
            health_checks: Mutex::new(0),
            fail_fen: Mutex::new(None),
        })
    }

    pub fn set_up(&self, up: bool) {
        self.up.store(up, Ordering::SeqCst);
    }

    pub fn set_mode(&self, mode: MaiaMode) {
        *self.mode.lock().unwrap() = mode;
    }

    pub fn sent(&self) -> Vec<DifficultyInput> {
        self.sent.lock().unwrap().clone()
    }

    pub fn sent_fens(&self) -> Vec<String> {
        self.sent().into_iter().map(|i| i.fen).collect()
    }

    pub fn clear(&self) {
        self.sent.lock().unwrap().clear();
    }

    pub fn difficulty() -> Difficulty {
        Difficulty {
            p_best: 0.25,
            p_ok: 0.75,
            model: MODEL.to_owned(),
        }
    }
}

impl DifficultySource for FakeMaia {
    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Vec<DifficultyOutcome>> {
        Box::pin(async move {
            if !self.up.load(Ordering::SeqCst) {
                return inputs
                    .iter()
                    .map(|_| DifficultyOutcome::PendingMaia {
                        reason: PendingReason::MaiaUnavailable,
                    })
                    .collect();
            }
            self.sent.lock().unwrap().extend(inputs.iter().cloned());
            let mode = *self.mode.lock().unwrap();
            let poisoned = self
                .fail_fen
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|f| inputs.iter().any(|i| &i.fen == f));
            if poisoned || mode == MaiaMode::GoesDown {
                if mode == MaiaMode::GoesDown {
                    self.set_up(false);
                }
                return inputs
                    .iter()
                    .map(|_| DifficultyOutcome::PendingMaia {
                        reason: PendingReason::MaiaUnavailable,
                    })
                    .collect();
            }
            inputs
                .iter()
                .map(|_| match mode {
                    MaiaMode::Ok => DifficultyOutcome::Ok {
                        difficulty: Self::difficulty(),
                    },
                    MaiaMode::Invalid => DifficultyOutcome::PendingMaia {
                        reason: PendingReason::InvalidResponse,
                    },
                    MaiaMode::FailsCalls | MaiaMode::GoesDown => DifficultyOutcome::PendingMaia {
                        reason: PendingReason::MaiaUnavailable,
                    },
                })
                .collect()
        })
    }

    fn healthy(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            *self.health_checks.lock().unwrap() += 1;
            self.up.load(Ordering::SeqCst)
        })
    }
}

/// A fake tablebase answering `wdl` for every position it is asked about.
pub struct FakeTb {
    pub wdl: Wdl,
    pub probed: Mutex<Vec<String>>,
}

impl FakeTb {
    pub fn new(wdl: Wdl) -> Arc<Self> {
        Arc::new(Self {
            wdl,
            probed: Mutex::new(Vec::new()),
        })
    }

    pub fn probed(&self) -> Vec<String> {
        self.probed.lock().unwrap().clone()
    }
}

impl TablebaseProbe for FakeTb {
    fn max_pieces(&self) -> u32 {
        5
    }

    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        Box::pin(async move {
            self.probed.lock().unwrap().push(fen.to_owned());
            Some(self.wdl)
        })
    }
}

/// Inserts `records` into `moves` for `game_id` (the re-enrichment sweep reads them back).
pub async fn insert_moves(
    pool: &sqlx::PgPool,
    game_id: uuid::Uuid,
    records: &[chess_core::MoveRecord],
) {
    for r in records {
        sqlx::query(
            "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
               win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
             VALUES ($1, $2, $3::color, $4, $5, $6, $7, $8, $9, $10, $11, $12::judgement, \
               $13::phase, $14, $15, $16)",
        )
        .bind(game_id)
        .bind(r.ply as i32)
        .bind(r.color.as_str())
        .bind(&r.san)
        .bind(&r.uci)
        .bind(&r.fen_before)
        .bind(&r.best_uci)
        .bind(r.win_before)
        .bind(r.win_after)
        .bind(r.delta_w)
        .bind(r.accuracy)
        .bind(r.judgement.map(|j| j.as_str()))
        .bind(r.phase.as_str())
        .bind(r.clock_before)
        .bind(r.time_spent)
        .bind(r.decided)
        .execute(pool)
        .await
        .expect("insert moves");
    }
}

/// A new account and game; returns the game id.
pub async fn seed_game(pool: &sqlx::PgPool) -> uuid::Uuid {
    let acc = testkit::insert_account(pool, "lichess", "hero").await;
    testkit::insert_game(pool, acc, testkit::GameSeed::default()).await
}

pub async fn count(pool: &sqlx::PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect("count")
}
