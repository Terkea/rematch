//! Shared chess types (final in Task 1; later tasks add behaviour, not fields).

/// Side to move, or the side that played a move. Serialises as `"white"` / `"black"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn as_str(self) -> &'static str {
        match self {
            Color::White => "white",
            Color::Black => "black",
        }
    }
}

/// Engine score from one side's point of view. JSON shape is {"cp":n} or {"mate":n}
/// (custom serde impl), matching games.lichess_evals written by TS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineScore {
    Cp(i32),
    Mate(i32),
}

/// Wire shape of [`EngineScore`]: exactly one of `cp` or `mate`.
#[derive(serde::Serialize, serde::Deserialize)]
struct EngineScoreRepr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cp: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mate: Option<i32>,
}

impl serde::Serialize for EngineScore {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let repr = match *self {
            EngineScore::Cp(cp) => EngineScoreRepr {
                cp: Some(cp),
                mate: None,
            },
            EngineScore::Mate(mate) => EngineScoreRepr {
                cp: None,
                mate: Some(mate),
            },
        };
        repr.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for EngineScore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match EngineScoreRepr::deserialize(deserializer)? {
            EngineScoreRepr {
                cp: Some(cp),
                mate: None,
            } => Ok(EngineScore::Cp(cp)),
            EngineScoreRepr {
                cp: None,
                mate: Some(mate),
            } => Ok(EngineScore::Mate(mate)),
            _ => Err(serde::de::Error::custom(
                "engine score must have exactly one of \"cp\" or \"mate\"",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionEval {
    pub score: EngineScore,
    pub best_uci: Option<String>,
    pub pv: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Judgement {
    Inaccuracy,
    Mistake,
    Blunder,
}

impl Judgement {
    pub fn as_str(self) -> &'static str {
        match self {
            Judgement::Inaccuracy => "inaccuracy",
            Judgement::Mistake => "mistake",
            Judgement::Blunder => "blunder",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Opening,
    Middlegame,
    Endgame,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Opening => "opening",
            Phase::Middlegame => "middlegame",
            Phase::Endgame => "endgame",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeClass {
    Bullet,
    Blitz,
    Rapid,
    Classical,
    Daily,
}

impl TimeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            TimeClass::Bullet => "bullet",
            TimeClass::Blitz => "blitz",
            TimeClass::Rapid => "rapid",
            TimeClass::Classical => "classical",
            TimeClass::Daily => "daily",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Terminal {
    Checkmate,
    Stalemate,
    Draw,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ply {
    pub ply: u32,
    pub color: Color,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub fen_after: String,
    pub clock_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedGame {
    pub headers: Vec<(String, String)>,
    pub start_fen: String,
    pub plies: Vec<Ply>,
    pub terminal: Option<Terminal>,
}

impl ParsedGame {
    /// Value of the PGN header `name`. When a header appears more than once the
    /// last occurrence wins (TS parity, where later tags overwrite earlier ones).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockInfo {
    pub base_seconds: f64,
    pub increment_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveRecord {
    pub ply: u32,
    pub color: Color,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub best_uci: Option<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub delta_w: f64,
    pub accuracy: f64,
    pub judgement: Option<Judgement>,
    pub phase: Phase,
    pub clock_before: Option<f64>,
    pub time_spent: Option<f64>,
    pub decided: bool,
}

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
