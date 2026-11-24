//! The coach chat's tool definitions (sorted by name).
//!
//! The schemas are written out by hand in the JSON Schema subset the sidecar converts to zod
//! (ADR 0062, `services/ai-sidecar/src/schema-to-zod.ts`): a top-level object with
//! `additionalProperties: false`; string (optionally `enum`), integer (with `minimum`/`maximum`)
//! and array-of-string properties. That converter has no `anyOf` or `null`, so an optional
//! property is left out of `required` (the converter makes it `.optional()`) instead of being
//! required and nullable. The executor ([`super::execute`]) re-validates every field in Rust and
//! treats a missing optional field and `null` alike.

use serde_json::{Value, json};

use crate::client::port::ToolDef;

pub const MAX_MOVES: usize = 4;
pub const MAX_PLY: u32 = 1000;
pub const MAX_ID_CHARS: usize = 64;
pub const MAX_FEN_CHARS: usize = 100;
pub const MAX_GAMES: u32 = 20;
pub const MAX_EXAMPLES: u32 = 10;
/// `find_examples`' `clockBelowSeconds` upper bound.
pub const MAX_CLOCK_SECONDS: u32 = 3600;
/// `find_examples`' `lastGames` upper bound.
pub const MAX_LAST_GAMES: u32 = 500;

const JUDGEMENTS: [&str; 3] = ["inaccuracy", "mistake", "blunder"];

/// The weakness ids `find_examples` knows (profile-core's `WeaknessKey`, contract ids). The
/// coaching context lists only the ranked ones, and with no band reference none is ranked, so
/// the model is told every id here (fix E).
const WEAKNESS_ID: &str = "A weakness id: one from the coaching context, or any of W1 hung     material, W2 missed opponent threat, W3 missed own tactic, W4 missed or allowed mate, W5     conversion failure (not winning a won position), W6 defence, W7 opening traps and     preparation, W8 endgame technique, W9a time trouble, W9b time-pressure collapse, W9c     rushing critical moves, W9d time misallocation, W10 positional drift, W11 easy-move errors,     W12 tilt.";

const GAME_ID: &str = "A game id from the coaching context, the anchor or an earlier tool result.";
const PLY: &str = "The half-move number of the position (1 = White's first move).";
const MOVES: &str = "Up to 4 moves in UCI (e2e4, e7e8q) to play from the position first.";

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn moves() -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": MOVES})
}

fn limit(max: u32) -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": max, "description": "How many results to return."})
}

fn win_bound(description: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "maximum": 100, "description": description})
}

fn ply() -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": MAX_PLY, "description": PLY})
}

fn string(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// Every chat tool, sorted by name.
pub fn chat_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "analyse_position",
            description: "Run a fresh engine check of a position after up to 4 moves, for \
                \"what if\" questions. Each check counts against the player's engine checks. \
                Cite the result only with the {{eng:...}} tokens in refs.",
            input_schema: object(
                json!({
                    "fen": string("The position as a FEN, for example fen or fenAfterPlayed from the anchor or get_position."),
                    "moves": moves(),
                }),
                &["fen", "moves"],
            ),
        },
        ToolDef {
            name: "find_examples",
            description: "Find positions from the player's own games: the player's moves, never                 the opponent's, in this chat's time class. Give exactly one selector:                 weaknessId, motif, lineKey, or a move filter (any of phase, judgement or                 minJudgement, standing, minWinBefore, maxWinBefore, minDeltaW,                 clockBelowSeconds, color, since, until, lastGames). Use the move filter for                 questions about the player's moves, such as \"a blunder in a winning                 endgame\" (phase endgame, judgement blunder, standing winning) or \"mistakes                 in time trouble\" (minJudgement mistake, clockBelowSeconds 60). A move filter                 gives one move per game, its biggest drop, newest game first (sort worst:                 biggest drop first), and without judgement, minJudgement or minDeltaW only                 inaccuracies, mistakes and blunders. Its result has searched: what was                 searched, with matchingMoves, matchingGames and gamesSearched. Cite positions                 with the tokens in refs.",
            input_schema: object(
                json!({
                    "weaknessId": string(WEAKNESS_ID),
                    "motif": string("A motif tag such as fork, discovered_attack or mate_in_2."),
                    "lineKey": string("An opening line key from the coaching context."),
                    "phase": {"type": "string", "enum": ["opening", "middlegame", "endgame"], "description": "Move filter: only moves in this phase of the game."},
                    "judgement": {"type": "string", "enum": JUDGEMENTS, "description": "Move filter: only moves with exactly this judgement."},
                    "minJudgement": {"type": "string", "enum": JUDGEMENTS, "description": "Move filter: only moves with this judgement or a worse one (mistake = mistakes and blunders). Not with judgement."},
                    "standing": {"type": "string", "enum": ["winning", "equal", "losing"], "description": "Move filter: the player's win chance before the move: winning is 70 or more, losing 30 or less, equal in between."},
                    "minWinBefore": win_bound("Move filter: the player's win chance before the move is at least this (0 to 100)."),
                    "maxWinBefore": win_bound("Move filter: the player's win chance before the move is at most this (0 to 100)."),
                    "minDeltaW": {"type": "integer", "minimum": 1, "maximum": 100, "description": "Move filter: the move lost at least this many win-chance points."},
                    "clockBelowSeconds": {"type": "integer", "minimum": 1, "maximum": MAX_CLOCK_SECONDS, "description": "Move filter: time trouble, the player had less than this many seconds on the clock."},
                    "color": {"type": "string", "enum": ["white", "black"], "description": "Move filter: only games where the player had this colour."},
                    "since": string("Move filter: only games played on or after this date, YYYY-MM-DD."),
                    "until": string("Move filter: only games played on or before this date, YYYY-MM-DD."),
                    "lastGames": {"type": "integer", "minimum": 1, "maximum": MAX_LAST_GAMES, "description": "Move filter: only the player's most recent analysed games, this many."},
                    "sort": {"type": "string", "enum": ["recent", "worst"], "description": "Move filter order: recent (default, newest game first) or worst (biggest drop first)."},
                    "limit": limit(MAX_EXAMPLES),
                }),
                &["limit"],
            ),
        },
        ToolDef {
            name: "get_eval",
            description: "Look up the stored engine evaluation of a position from the \
                player's game, optionally after up to 4 moves. Never runs the engine; a \
                position that was not analysed is reported as unavailable.",
            input_schema: object(
                json!({"gameId": string(GAME_ID), "ply": ply(), "moves": moves()}),
                &["gameId", "ply", "moves"],
            ),
        },
        ToolDef {
            name: "get_games",
            description: "List the player's recent games, newest first, optionally filtered \
                by result, opening name or start date.",
            input_schema: object(
                json!({
                    "result": {"type": "string", "enum": ["win", "loss", "draw"], "description": "Only games with this result for the player."},
                    "openingContains": string("Only games whose opening name contains this text."),
                    "since": string("Only games played on or after this date, YYYY-MM-DD."),
                    "limit": limit(MAX_GAMES),
                }),
                &["limit"],
            ),
        },
        ToolDef {
            name: "get_opening_line",
            description: "Get one of the player's opening lines with its computed verdict.",
            input_schema: object(
                json!({"lineKey": string("An opening line key from the coaching context.")}),
                &["lineKey"],
            ),
        },
        ToolDef {
            name: "get_position",
            description: "Get one position from the player's game: the move played, the \
                engine's best move and line, win chances and clock. Cite it with the tokens \
                in refs.",
            input_schema: object(
                json!({"gameId": string(GAME_ID), "ply": ply()}),
                &["gameId", "ply"],
            ),
        },
        ToolDef {
            name: "get_weakness",
            description: "Get one of the player's weaknesses with its rates and example \
                positions.",
            input_schema: object(
                json!({"weaknessId": string("A weakness id from the coaching context.")}),
                &["weaknessId"],
            ),
        },
    ]
}
