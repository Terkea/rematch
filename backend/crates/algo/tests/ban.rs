//! No unseeded randomness and no wall clock in the maths crates (ADR 0104, issue #53).

use algo::ban::{ALLOWED, SCANNED, scan};
use algo::repo_root;

#[test]
fn the_maths_crates_have_no_rng_or_clock_outside_the_allow_list() {
    let v = scan(&repo_root(), &SCANNED, &ALLOWED).expect("scan");
    assert!(v.is_empty(), "banned calls: {v:#?}");
}

#[test]
fn every_allow_list_entry_is_still_used() {
    for (file, call, reason) in ALLOWED {
        assert!(!reason.is_empty());
        let text = std::fs::read_to_string(repo_root().join(file)).expect(file);
        assert!(
            text.contains(call),
            "{file} no longer uses {call}: drop it from ALLOWED"
        );
    }
}

#[test]
fn a_planted_call_fails_the_scan() {
    let root = std::env::temp_dir().join(format!("algo-ban-{}", std::process::id()));
    let dir = root.join("crate/src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("maths.rs"),
        "// rand:: in a comment is fine\npub fn f() -> u64 { rand::random() }\n\
         pub fn g() { let _ = time::OffsetDateTime::now_utc(); }\n",
    )
    .unwrap();
    let v = scan(&root, &["crate/src"], &[]).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let found: Vec<(usize, &str)> = v.iter().map(|x| (x.line, x.call)).collect();
    assert_eq!(found, vec![(2, "rand::"), (3, "OffsetDateTime::now_utc")]);
}
