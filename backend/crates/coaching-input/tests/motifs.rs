//! The player-visible motif allowlist is EXP-0007's decision (#122, ADR 0112): exactly the tags
//! its section 9 trusted, plus the tags it could not score, and none of the tags it hid. Clearing
//! a hidden tag later (EXP-0019 to EXP-0021) is one line in `PLAYER_VISIBLE_MOTIFS` and this
//! test's source moving to that experiment's results.

use std::collections::BTreeSet;

use coaching_input::motifs::{
    PLAYER_VISIBLE_MOTIFS, PLAYER_VISIBLE_MOTIFS_SOURCE, is_player_visible,
    is_player_visible_contract_tag,
};
use serde_json::Value;

fn exp_0007() -> Value {
    let path = format!(
        "{}/../../../lab/experiments/EXP-0007/results.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn names(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .expect("an array of tags")
        .iter()
        .map(|t| t.as_str().expect("a tag").to_owned())
        .collect()
}

#[test]
fn the_allowlist_is_exp_0007s_decision() {
    let r = exp_0007();
    assert_eq!(PLAYER_VISIBLE_MOTIFS_SOURCE, "EXP-0007");
    let decisions = &r["decisions"];
    let trusted = names(&decisions["trusted"]);
    let unscored: BTreeSet<String> = r["unscored"]
        .as_object()
        .expect("unscored tags")
        .keys()
        .cloned()
        .collect();
    let hidden: BTreeSet<String> = ["hidden pending a human audit", "hidden until fixed"]
        .iter()
        .flat_map(|k| names(&decisions[*k]))
        .chain(names(&decisions["needs fixing"]))
        .collect();
    let visible: BTreeSet<String> = PLAYER_VISIBLE_MOTIFS
        .iter()
        .map(|t| (*t).to_owned())
        .collect();
    assert_eq!(visible.len(), PLAYER_VISIBLE_MOTIFS.len(), "no duplicates");
    assert_eq!(
        visible,
        trusted.union(&unscored).cloned().collect::<BTreeSet<_>>()
    );
    assert!(
        visible.is_disjoint(&hidden),
        "{visible:?} names a hidden tag"
    );
    assert_eq!(
        hidden,
        [
            "backRankMate",
            "deflection",
            "hangingPiece",
            "pin",
            "sacrifice",
            "skewer"
        ]
        .iter()
        .map(|t| (*t).to_owned())
        .collect::<BTreeSet<_>>(),
        "EXP-0007 hid these six (#122)"
    );
}

#[test]
fn a_tag_is_looked_up_in_either_form() {
    assert!(is_player_visible("fork"));
    assert!(is_player_visible("discoveredAttack"));
    assert!(is_player_visible("mateIn4Plus"));
    assert!(!is_player_visible("pin"));
    assert!(!is_player_visible("hangingPiece"));
    assert!(!is_player_visible("nope"));
    assert!(is_player_visible_contract_tag("discovered_attack"));
    assert!(is_player_visible_contract_tag("mate_in_4_plus"));
    assert!(is_player_visible_contract_tag("smothered_mate"));
    assert!(!is_player_visible_contract_tag("hanging_piece"));
    assert!(!is_player_visible_contract_tag("back_rank_mate"));
    assert!(!is_player_visible_contract_tag("discoveredAttack"));
}
