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
            description: "Find other positions from the player's own games for one weakness, \
                one motif tag or one opening line. Give exactly one of weaknessId, motif and \
                lineKey. Cite positions with the tokens in refs.",
            input_schema: object(
                json!({
                    "weaknessId": string("A weakness id from the coaching context."),
                    "motif": string("A motif tag such as fork or hung_piece."),
                    "lineKey": string("An opening line key from the coaching context."),
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
