//! The CI guard's Rust half (ADR 0104, issue #50): the committed `algorithm/manifest.json` must be
//! exactly what the code builds. Regenerate with `cargo run -p cli -- algo manifest --write`.

use algo::{MANIFEST_PATH, Manifest, build, diff, repo_root};

fn committed() -> (String, Manifest) {
    let text = std::fs::read_to_string(repo_root().join(MANIFEST_PATH)).expect("manifest.json");
    let m = Manifest::from_value(&serde_json::from_str(&text).expect("valid JSON"))
        .expect("id matches content");
    (text, m)
}

#[test]
fn the_committed_manifest_matches_the_code() {
    let built = build(&repo_root()).expect("manifest builds");
    let (text, m) = committed();
    let changes = diff::render(&diff::diff(&m.to_value(), &built.to_value()));
    assert!(
        changes.is_empty(),
        "algorithm/manifest.json differs from the code (committed -> code):\n  {}\n\
         Run `cargo run -p cli -- algo manifest --write`, bump algo::VERSION by the policy and \
         add an algorithm/CHANGELOG.md entry.",
        changes.join("\n  ")
    );
    assert_eq!(
        text,
        built.to_pretty(),
        "manifest.json is not in its generated form"
    );
}

#[test]
fn common_reads_the_same_id_as_the_code_builds() {
    let built = build(&repo_root()).expect("manifest builds");
    assert_eq!(common::algorithm::id(), built.id());
    assert_eq!(common::algorithm::manifest(), built.to_pretty());
}

#[test]
fn the_manifest_names_the_engine_settings_and_the_new_versions() {
    let m = build(&repo_root()).expect("manifest builds");
    let c = &m.components;
    assert_eq!(c["facts"]["engine_protocol"], engine::ANALYSIS_PROTOCOL);
    // EXP-0008 (ADR 0113): the main search at 1 million nodes, the MultiPV probe kept at 300,000.
    assert_eq!(c["facts"]["engine_protocol"], "sf19-n1m-v1");
    assert_eq!(c["facts"]["engine_nodes"], 1_000_000);
    assert_eq!(c["facts"]["enrichment"]["multipv_nodes"], 300_000);
    assert_eq!(c["facts"]["enrichment"]["multipv_protocol"], "sf19-n300k-mpv3-v1");
    assert_eq!(c["facts"]["engine_options"]["threads"], 1);
    assert_eq!(c["facts"]["engine_options"]["hash_mb"], 16);
    assert_eq!(c["facts"]["stockfish"]["release"], "sf_19");
    assert_eq!(
        c["facts"]["stockfish"]["sha256"],
        "9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f"
    );
    assert_eq!(c["recommendation"]["ranking_version"], "rank-v1");
    assert_eq!(c["recommendation"]["trainer_version"], "trainer-v1");
    assert_eq!(c["recommendation"]["focus_version"], "focus-v1");
    assert_eq!(c["facts"]["maia"]["package"], "maia2==0.11.0");
    assert_eq!(c["facts"]["phase_metrics_version"], 3);
}

#[test]
fn the_changelog_has_an_entry_for_the_committed_id() {
    let (_, m) = committed();
    let log = std::fs::read_to_string(repo_root().join("algorithm/CHANGELOG.md")).unwrap();
    assert!(
        log.lines().any(|l| l == format!("## {}", m.id())),
        "algorithm/CHANGELOG.md has no `## {}` heading",
        m.id()
    );
}
