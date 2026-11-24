//! The motif tags a player may be told about (#122, ADR 0112).
//!
//! The tagger computes 16 tags on every engine line and the facts keep all of them
//! (`move_features`, `cli lab tag-motifs`), so later experiments can keep measuring each one.
//! Only the tags in [`PLAYER_VISIBLE_MOTIFS`] reach a player or the coach: the missed and
//! allowed motif series, the KPI rows, the timeline, the coaching input's tags, the chat's
//! positions and the trainer's pattern rates. EXP-0007 measured the tagger against Lichess's
//! puzzle themes and trusted `fork`, `discoveredAttack`, `promotion`, `smotheredMate` and the
//! five mate-length tags; it hid `hangingPiece`, `pin`, `skewer`, `deflection`, `sacrifice` and
//! `backRankMate`, and had no theme to score `matingNet` against, so that tag stays as it was.
//!
//! The list lives here because the profile, the coaching input and the coach (`ai`) all depend
//! on this crate. The algorithm manifest names it (`profile.player_visible_motifs`), so changing
//! it is a new algorithm version. `tests/motifs.rs` pins it to EXP-0007's `results.json`.

/// The experiment whose decision [`PLAYER_VISIBLE_MOTIFS`] is.
pub const PLAYER_VISIBLE_MOTIFS_SOURCE: &str = "EXP-0007";

/// The player-visible motif tags, in the tagger's camelCase (`MotifTag::as_str`).
pub const PLAYER_VISIBLE_MOTIFS: [&str; 10] = [
    "fork",
    "discoveredAttack",
    "promotion",
    "mate",
    "mateIn1",
    "mateIn2",
    "mateIn3",
    "mateIn4Plus",
    // Not scored by EXP-0007 (no Lichess theme to compare with); kept as before.
    "matingNet",
    "smotheredMate",
];

/// Whether a camelCase tag (`discoveredAttack`) may be named to a player.
pub fn is_player_visible(tag: &str) -> bool {
    PLAYER_VISIBLE_MOTIFS.contains(&tag)
}

/// The contract's snake_case form of a camelCase tag: a word per capital, digits as their own
/// word (`mateIn4Plus` is `mate_in_4_plus`).
fn snake(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    let mut prev_digit = false;
    for c in camel.chars() {
        let digit = c.is_ascii_digit();
        if c.is_ascii_uppercase() || (digit && !prev_digit && !out.is_empty()) {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
        prev_digit = digit;
    }
    out
}

/// Whether a contract snake_case tag (`discovered_attack`, `ExemplarPosition.tags`) may be named
/// to a player.
pub fn is_player_visible_contract_tag(tag: &str) -> bool {
    PLAYER_VISIBLE_MOTIFS.iter().any(|t| snake(t) == tag)
}
