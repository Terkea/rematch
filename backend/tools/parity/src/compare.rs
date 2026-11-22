//! Snapshot loading and the comparison rules.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct GameRow {
    pub platform: String,
    pub platform_game_id: String,
    pub analysis_status: String,
    pub analysis_protocol: Option<String>,
    pub white_accuracy: Option<f64>,
    pub black_accuracy: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveRow {
    pub ply: i32,
    pub color: String,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub best_uci: Option<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub delta_w: f64,
    pub accuracy: f64,
    pub judgement: Option<String>,
    pub phase: String,
    pub clock_before: Option<f64>,
    pub time_spent: Option<f64>,
    pub decided: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvalRow {
    pub fen_key: String,
    pub protocol: String,
    pub score_cp: Option<i32>,
    pub score_mate: Option<i32>,
    pub best_uci: Option<String>,
    pub pv: Vec<String>,
    pub source: String,
}

type GameKey = (String, String);

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub games: BTreeMap<GameKey, (GameRow, Vec<MoveRow>)>,
    pub evals: BTreeMap<GameKey, EvalRow>,
}

impl Snapshot {
    pub fn insert_game(&mut self, g: GameRow, moves: Vec<MoveRow>) {
        self.games
            .insert((g.platform.clone(), g.platform_game_id.clone()), (g, moves));
    }
    pub fn insert_eval(&mut self, e: EvalRow) {
        self.evals
            .insert((e.fen_key.clone(), e.protocol.clone()), e);
    }
    pub fn game_mut(&mut self, platform: &str, id: &str) -> &mut GameRow {
        &mut self.entry(platform, id).0
    }
    pub fn moves_mut(&mut self, platform: &str, id: &str) -> &mut Vec<MoveRow> {
        &mut self.entry(platform, id).1
    }
    pub fn eval_mut(&mut self, fen_key: &str, protocol: &str) -> &mut EvalRow {
        self.evals
            .get_mut(&(fen_key.to_owned(), protocol.to_owned()))
            .expect("no such eval")
    }
    fn entry(&mut self, platform: &str, id: &str) -> &mut (GameRow, Vec<MoveRow>) {
        self.games
            .get_mut(&(platform.to_owned(), id.to_owned()))
            .expect("no such game")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub games: usize,
    pub moves: usize,
    pub evals: usize,
    pub mismatches: Vec<String>,
}

impl Report {
    /// The run's last line, e.g. `parity: 3 games, 120 moves, 90 evals compared, 0 mismatches`.
    pub fn summary(&self) -> String {
        format!(
            "parity: {} games, {} moves, {} evals compared, {} mismatches",
            self.games,
            self.moves,
            self.evals,
            self.mismatches.len()
        )
    }
}

/// `win_before`, `win_after`, `delta_w`.
pub const WIN_TOLERANCE: f64 = 1e-9;
/// Move `accuracy` and the games' `white_accuracy`/`black_accuracy`.
pub const ACCURACY_TOLERANCE: f64 = 1e-6;

/// One printable cell value.
trait Cell {
    fn show(&self) -> String;
}
impl Cell for String {
    fn show(&self) -> String {
        self.clone()
    }
}
impl Cell for bool {
    fn show(&self) -> String {
        self.to_string()
    }
}
impl Cell for f64 {
    fn show(&self) -> String {
        self.to_string()
    }
}
impl Cell for i32 {
    fn show(&self) -> String {
        self.to_string()
    }
}
impl Cell for Vec<String> {
    fn show(&self) -> String {
        format!("{{{}}}", self.join(","))
    }
}
impl<T: Cell> Cell for Option<T> {
    fn show(&self) -> String {
        self.as_ref().map_or_else(|| "NULL".to_owned(), Cell::show)
    }
}

struct Out<'a> {
    prefix: String,
    out: &'a mut Vec<String>,
}

impl Out<'_> {
    fn exact<T: Cell + PartialEq>(&mut self, col: &str, a: &T, b: &T) {
        if a != b {
            self.push(col, a, b);
        }
    }
    fn near(&mut self, col: &str, a: f64, b: f64, tol: f64) {
        self.near_opt(col, Some(a), Some(b), tol);
    }
    fn near_opt(&mut self, col: &str, a: Option<f64>, b: Option<f64>, tol: f64) {
        let same = match (a, b) {
            (Some(x), Some(y)) => (x - y).abs() <= tol,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.push(col, &a, &b);
        }
    }
    fn push(&mut self, col: &str, a: &dyn Cell, b: &dyn Cell) {
        self.out.push(format!(
            "{} {col}: {} != {}",
            self.prefix,
            a.show(),
            b.show()
        ));
    }
}

fn presence<T>(a: Option<&T>, b: Option<&T>) -> Option<String> {
    let p = |x: Option<&T>| if x.is_some() { "present" } else { "missing" };
    match (a, b) {
        (Some(_), Some(_)) => None,
        _ => Some(format!("{} != {}", p(a), p(b))),
    }
}

/// Compares two snapshots by `(platform, platform_game_id)`, `ply` and `(fen_key, protocol)`.
pub fn diff(a: &Snapshot, b: &Snapshot) -> Report {
    let mut r = Report::default();
    let keys: std::collections::BTreeSet<&GameKey> = a.games.keys().chain(b.games.keys()).collect();
    for key in keys {
        r.games += 1;
        let (ga, gb) = (a.games.get(key), b.games.get(key));
        let id = &key.1;
        if let Some(p) = presence(ga, gb) {
            r.mismatches.push(format!("{id} game: {p}"));
            continue;
        }
        let (Some((ga, ma)), Some((gb, mb))) = (ga, gb) else {
            continue;
        };
        let mut o = Out {
            prefix: format!("{id} game"),
            out: &mut r.mismatches,
        };
        o.exact("analysis_status", &ga.analysis_status, &gb.analysis_status);
        o.exact(
            "analysis_protocol",
            &ga.analysis_protocol,
            &gb.analysis_protocol,
        );
        o.near_opt(
            "white_accuracy",
            ga.white_accuracy,
            gb.white_accuracy,
            ACCURACY_TOLERANCE,
        );
        o.near_opt(
            "black_accuracy",
            ga.black_accuracy,
            gb.black_accuracy,
            ACCURACY_TOLERANCE,
        );
        r.moves += ma.len().max(mb.len());
        if ma.len() != mb.len() {
            o.prefix = format!("{id} moves");
            o.push("count", &(ma.len() as i32), &(mb.len() as i32));
            continue;
        }
        for (x, y) in ma.iter().zip(mb) {
            o.prefix = format!("{id} ply {}", x.ply);
            o.exact("ply", &x.ply, &y.ply);
            o.exact("color", &x.color, &y.color);
            o.exact("san", &x.san, &y.san);
            o.exact("uci", &x.uci, &y.uci);
            o.exact("fen_before", &x.fen_before, &y.fen_before);
            o.exact("best_uci", &x.best_uci, &y.best_uci);
            o.near("win_before", x.win_before, y.win_before, WIN_TOLERANCE);
            o.near("win_after", x.win_after, y.win_after, WIN_TOLERANCE);
            o.near("delta_w", x.delta_w, y.delta_w, WIN_TOLERANCE);
            o.near("accuracy", x.accuracy, y.accuracy, ACCURACY_TOLERANCE);
            o.exact("judgement", &x.judgement, &y.judgement);
            o.exact("phase", &x.phase, &y.phase);
            o.exact("clock_before", &x.clock_before, &y.clock_before);
            o.exact("time_spent", &x.time_spent, &y.time_spent);
            o.exact("decided", &x.decided, &y.decided);
        }
    }
    let keys: std::collections::BTreeSet<&GameKey> = a.evals.keys().chain(b.evals.keys()).collect();
    for key in keys {
        r.evals += 1;
        let prefix = format!("eval {} [{}]", key.0, key.1);
        let (ea, eb) = (a.evals.get(key), b.evals.get(key));
        if let Some(p) = presence(ea, eb) {
            r.mismatches.push(format!("{prefix}: {p}"));
            continue;
        }
        let (Some(x), Some(y)) = (ea, eb) else {
            continue;
        };
        let mut o = Out {
            prefix,
            out: &mut r.mismatches,
        };
        o.exact("score_cp", &x.score_cp, &y.score_cp);
        o.exact("score_mate", &x.score_mate, &y.score_mate);
        o.exact("best_uci", &x.best_uci, &y.best_uci);
        o.exact("pv", &x.pv, &y.pv);
        o.exact("source", &x.source, &y.source);
    }
    r
}

/// Reads every game (with its moves in ply order) and every cached eval.
pub async fn load(pool: &sqlx::PgPool) -> anyhow::Result<Snapshot> {
    use sqlx::Row;
    let mut s = Snapshot::default();
    let games = sqlx::query(
        "SELECT id, platform::text AS platform, platform_game_id, \
         analysis_status::text AS analysis_status, analysis_protocol, \
         white_accuracy::float8 AS white_accuracy, black_accuracy::float8 AS black_accuracy \
         FROM games",
    )
    .fetch_all(pool)
    .await?;
    let mut by_id = std::collections::HashMap::new();
    for r in games {
        let g = GameRow {
            platform: r.try_get("platform")?,
            platform_game_id: r.try_get("platform_game_id")?,
            analysis_status: r.try_get("analysis_status")?,
            analysis_protocol: r.try_get("analysis_protocol")?,
            white_accuracy: r.try_get("white_accuracy")?,
            black_accuracy: r.try_get("black_accuracy")?,
        };
        let id: uuid::Uuid = r.try_get("id")?;
        by_id.insert(id, (g.platform.clone(), g.platform_game_id.clone()));
        s.insert_game(g, Vec::new());
    }
    let moves = sqlx::query(
        "SELECT game_id, ply, color::text AS color, san, uci, fen_before, best_uci, win_before, \
         win_after, delta_w, accuracy, judgement::text AS judgement, phase::text AS phase, \
         clock_before, time_spent, decided FROM moves ORDER BY game_id, ply",
    )
    .fetch_all(pool)
    .await?;
    for r in moves {
        let game: uuid::Uuid = r.try_get("game_id")?;
        let m = MoveRow {
            ply: r.try_get("ply")?,
            color: r.try_get("color")?,
            san: r.try_get("san")?,
            uci: r.try_get("uci")?,
            fen_before: r.try_get("fen_before")?,
            best_uci: r.try_get("best_uci")?,
            win_before: r.try_get("win_before")?,
            win_after: r.try_get("win_after")?,
            delta_w: r.try_get("delta_w")?,
            accuracy: r.try_get("accuracy")?,
            judgement: r.try_get("judgement")?,
            phase: r.try_get("phase")?,
            clock_before: r.try_get("clock_before")?,
            time_spent: r.try_get("time_spent")?,
            decided: r.try_get("decided")?,
        };
        if let Some(key) = by_id.get(&game)
            && let Some(entry) = s.games.get_mut(key)
        {
            entry.1.push(m);
        }
    }
    let evals = sqlx::query(
        "SELECT fen_key, protocol, score_cp, score_mate, best_uci, pv, source::text AS source \
         FROM position_evals",
    )
    .fetch_all(pool)
    .await?;
    for r in evals {
        s.insert_eval(EvalRow {
            fen_key: r.try_get("fen_key")?,
            protocol: r.try_get("protocol")?,
            score_cp: r.try_get("score_cp")?,
            score_mate: r.try_get("score_mate")?,
            best_uci: r.try_get("best_uci")?,
            pv: r.try_get("pv")?,
            source: r.try_get("source")?,
        });
    }
    Ok(s)
}
