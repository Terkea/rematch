//! `cli algo` (ADR 0105): show and diff manifests from this build, files and the database.

use std::sync::Mutex;

use cli::algo_cmd::run_algo;
use sqlx::PgPool;

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

async fn run(a: &[&str], pool: Option<&PgPool>) -> (i32, String, String) {
    let out = Mutex::new(Vec::<String>::new());
    let err = Mutex::new(Vec::<String>::new());
    let code = run_algo(
        &argv(a),
        pool,
        &|l| out.lock().unwrap().push(l.to_owned()),
        &|l| err.lock().unwrap().push(l.to_owned()),
    )
    .await;
    (
        code,
        out.into_inner().unwrap().join("\n"),
        err.into_inner().unwrap().join("\n"),
    )
}

fn temp_manifest(name: &str, nodes: u64) -> std::path::PathBuf {
    let mut v: serde_json::Value = serde_json::from_str(common::algorithm::manifest()).unwrap();
    v["components"]["facts"]["engine_nodes"] = nodes.into();
    let m = algo::Manifest::from_value(&serde_json::json!({
        "version": v["version"], "components": v["components"]
    }))
    .unwrap();
    let p = std::env::temp_dir().join(format!("algo-{name}-{}.json", std::process::id()));
    std::fs::write(&p, m.to_pretty()).unwrap();
    p
}

#[tokio::test]
async fn show_prints_this_builds_id() {
    let (code, out, _) = run(&["algo", "show"], None).await;
    assert_eq!(code, 0);
    assert!(out.starts_with(common::algorithm::id()), "{out}");
    assert!(out.contains("rank-v1"), "{out}");
    let (code, json, _) = run(&["algo", "show", "--json"], None).await;
    assert_eq!(code, 0);
    assert_eq!(json.trim_end(), common::algorithm::manifest().trim_end());
}

#[tokio::test]
async fn diff_names_the_changed_component_and_exits_3() {
    let a = temp_manifest("a", 300_000);
    let b = temp_manifest("b", 500_000);
    let (code, out, _) = run(
        &["algo", "diff", a.to_str().unwrap(), b.to_str().unwrap()],
        None,
    )
    .await;
    assert_eq!(code, 3, "{out}");
    assert!(
        out.contains("components.facts.engine_nodes: 300000 -> 500000"),
        "{out}"
    );
    let (code, json, _) = run(&["algo", "diff", "current", "current", "--json"], None).await;
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["changes"], serde_json::json!([]));
    assert_eq!(v["a"], common::algorithm::id());
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

#[tokio::test]
async fn an_id_without_a_database_is_an_error() {
    let (code, _, err) = run(&["algo", "show", "algo-9.9.9+000000000000"], None).await;
    assert_eq!(code, 1);
    assert!(err.contains("DATABASE_URL"), "{err}");
    let (code, _, err) = run(&["algo", "nope"], None).await;
    assert_eq!(code, 1);
    assert!(err.starts_with("usage:"), "{err}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn show_reads_a_manifest_from_algorithm_versions(pool: PgPool) {
    db::algorithm::register(&pool).await.unwrap();
    let id = common::algorithm::id();
    let (code, out, err) = run(&["algo", "show", id], Some(&pool)).await;
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with(id), "{out}");
    let (code, out, _) = run(&["algo", "diff", id, "current"], Some(&pool)).await;
    assert_eq!(code, 0, "{out}");
}
