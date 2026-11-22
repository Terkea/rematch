//! Contract constants: `LIMITS` and the other zod bounds of `packages/ai/src/input/schema.ts`.
//!
//! The schemars attributes in `types.rs` and the checks in `validate.rs` both read these, so the
//! generated schema and `CoachingInput::validate` cannot drift apart.

/// The version the builder writes (ADR 0085): v1 plus `byColor` and `benchmark`.
pub const INPUT_VERSION: &str = "coaching-input-v2";
/// zod `z.literal("coaching-input-v1")`: the M3 contract, still accepted when parsing.
pub const INPUT_VERSION_V1: &str = "coaching-input-v1";
/// v2: games a colour needs (in its window) before its numbers are stated (the honesty rule of
/// the colour split, ADR 0082; the web's `KPI_MIN_GAMES`).
pub const MIN_COLOR_GAMES: u32 = 10;

/// zod `LIMITS`: size bounds applied at the trust boundary. Lengths count Unicode code points, as
/// zod 4.6 (`util.codePointLength`) and JSON Schema's `maxLength` do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub pv_plies: usize,
    pub tags: usize,
    pub tag_chars: usize,
    pub metrics: usize,
    pub weaknesses: usize,
    pub strengths: usize,
    pub openings: usize,
    pub changes: usize,
    pub alternatives: usize,
    pub candidate_causes: usize,
    pub moves_san: usize,
    pub san_chars: usize,
    pub label_chars: usize,
    pub description_chars: usize,
    pub id_chars: usize,
    pub exemplars_per_item: usize,
    pub examples_per_opening: usize,
    pub examples_per_change: usize,
    /// v2: weakness rows per colour in `byColor`.
    pub color_weaknesses: usize,
}

pub const LIMITS: Limits = Limits {
    pv_plies: 16,
    tags: 12,
    tag_chars: 40,
    metrics: 80,
    weaknesses: 24,
    strengths: 12,
    openings: 30,
    changes: 20,
    alternatives: 5,
    candidate_causes: 6,
    moves_san: 40,
    san_chars: 12,
    label_chars: 120,
    description_chars: 600,
    id_chars: 64,
    exemplars_per_item: 5,
    examples_per_opening: 3,
    examples_per_change: 6,
    color_weaknesses: 5,
};

/// zod `UCI_RE`.
pub const UCI_PATTERN: &str = "^[a-h][1-8][a-h][1-8][qrbn]?$";
/// zod `WeaknessId`: W1..W12, optionally with a variant letter a-d.
pub const WEAKNESS_ID_PATTERN: &str = "^W(1[0-2]|[1-9])[a-d]?$";
pub const WEAKNESS_ID_MAX_CHARS: usize = 5;
/// `exemplarFields.fen`.
pub const FEN_MAX_CHARS: usize = 100;
/// `OpeningVerdict.eco`.
pub const ECO_MAX_CHARS: usize = 8;
/// `IsoDatetime = z.iso.datetime().max(40)`.
pub const DATETIME_MAX_CHARS: usize = 40;

/// zod 4's `z.uuid()` pattern, as exported in `ts-schema.json`.
pub const UUID_PATTERN: &str = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$";
/// zod 4's `z.iso.datetime()` pattern (UTC `Z`, any fraction), as exported in `ts-schema.json`.
pub const DATETIME_PATTERN: &str = r"^(?:(?:\d\d[2468][048]|\d\d[13579][26]|\d\d0[48]|[02468][048]00|[13579][26]00)-02-29|\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\d|30)|(?:02)-(?:0[1-9]|1\d|2[0-8])))T(?:(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z))$";
/// zod 4's `z.iso.date()` pattern, as exported in `ts-schema.json`.
pub const DATE_PATTERN: &str = r"^(?:(?:\d\d[2468][048]|\d\d[13579][26]|\d\d0[48]|[02468][048]00|[13579][26]00)-02-29|\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\d|30)|(?:02)-(?:0[1-9]|1\d|2[0-8])))$";

/// `exemplarFields.ply` and `OpeningVerdict.deviationPly`: 1..=1000.
pub const MAX_PLY: u32 = 1000;
/// zod `Count`: an integer 0..=100 000 000.
pub const MAX_COUNT: u32 = 100_000_000;
/// `CoachingInput.rating`: 0..=4000.
pub const MAX_RATING: u16 = 4000;
/// `TimeControl.baseSeconds`: 0..=10 000 000.
pub const MAX_BASE_SECONDS: u32 = 10_000_000;
/// `TimeControl.incrementSeconds`: 0..=100 000.
pub const MAX_INCREMENT_SECONDS: u32 = 100_000;
/// `Score.cp`: -100 000..=100 000.
pub const MAX_CP: i32 = 100_000;
/// `Score.mate`: -500..=500.
pub const MAX_MATE: i32 = 500;
/// `RankedWeakness.excessEplPerGame`: 0..=100.
pub const MAX_EXCESS_EPL: u32 = 100;
/// `RankedWeakness.priority`: 0..=1000.
pub const MAX_PRIORITY: u32 = 1000;
