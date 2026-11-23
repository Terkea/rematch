//! `cli algo` (ADR 0105): the algorithm manifest.
//!
//! - `cli algo manifest [--write] [--root DIR]` builds the manifest from the code in a checkout
//!   (default: the one this binary was built in) and prints it, or writes
//!   `algorithm/manifest.json`.
//! - `cli algo show [ID|FILE] [--json]` prints a manifest: this build's (no argument), one in
//!   `algorithm_versions` (an id, needs `DATABASE_URL`), or a file.
//! - `cli algo diff A B` prints the components that differ. Each side is `current`, an id in the
//!   database, or a manifest file. Exits 0 when they are the same and 3 when they differ.

use std::path::{Path, PathBuf};

use algo::Manifest;
use sqlx::PgPool;

pub const ALGO_USAGE: &str = "usage: cli algo manifest [--write] [--root DIR] | cli algo show [ID|FILE] [--json] | cli algo diff A B";

/// Where a manifest comes from.
#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    Current,
    File(PathBuf),
    Id(String),
}

pub fn source_of(arg: &str) -> Source {
    if arg == "current" {
        Source::Current
    } else if arg.starts_with("algo-") && !Path::new(arg).exists() {
        Source::Id(arg.to_owned())
    } else {
        Source::File(PathBuf::from(arg))
    }
}

fn current() -> anyhow::Result<Manifest> {
    Manifest::from_value(&serde_json::from_str(common::algorithm::manifest())?)
}

async fn load(src: &Source, pool: Option<&PgPool>) -> anyhow::Result<Manifest> {
    match src {
        Source::Current => current(),
        Source::File(p) => algo::read(p),
        Source::Id(id) => {
            let pool = pool.ok_or_else(|| {
                anyhow::anyhow!("{id}: reading an id needs DATABASE_URL (or pass a file)")
            })?;
            let v: Option<serde_json::Value> =
                sqlx::query_scalar("SELECT manifest FROM algorithm_versions WHERE id = $1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await?;
            let v = v.ok_or_else(|| anyhow::anyhow!("{id} is not in algorithm_versions"))?;
            Manifest::from_value(&v)
        }
    }
}

/// True when the command reads the database (an id argument).
pub fn needs_db(argv: &[String]) -> bool {
    argv.iter()
        .skip(2)
        .any(|a| matches!(source_of(a), Source::Id(_)))
}

pub async fn run_algo(
    argv: &[String],
    pool: Option<&PgPool>,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let args: Vec<&str> = argv.iter().skip(1).map(String::as_str).collect();
    let result = match args.as_slice() {
        ["manifest", rest @ ..] => manifest(rest, print),
        ["show", rest @ ..] => show(rest, pool, print).await,
        ["diff", a, b] => diff(a, b, pool, print).await,
        _ => Err(anyhow::anyhow!(ALGO_USAGE)),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            print_error(&format!("{e:#}"));
            1
        }
    }
}

fn manifest(rest: &[&str], print: &dyn Fn(&str)) -> anyhow::Result<i32> {
    let mut write = false;
    let mut root = algo::repo_root();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match *a {
            "--write" => write = true,
            "--root" => {
                root = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!(ALGO_USAGE))?)
            }
            _ => anyhow::bail!(ALGO_USAGE),
        }
    }
    let m = algo::build(&root)?;
    if write {
        let path = root.join(algo::MANIFEST_PATH);
        std::fs::write(&path, m.to_pretty())?;
        print(&format!("wrote {} ({})", algo::MANIFEST_PATH, m.id()));
    } else {
        print(m.to_pretty().trim_end());
    }
    Ok(0)
}

async fn show(rest: &[&str], pool: Option<&PgPool>, print: &dyn Fn(&str)) -> anyhow::Result<i32> {
    let json = rest.contains(&"--json");
    let args: Vec<&&str> = rest.iter().filter(|a| **a != "--json").collect();
    let src = match args.as_slice() {
        [] => Source::Current,
        [a] => source_of(a),
        _ => anyhow::bail!(ALGO_USAGE),
    };
    let m = load(&src, pool).await?;
    if json {
        print(m.to_pretty().trim_end());
        return Ok(0);
    }
    print(&m.id());
    let c = &m.components;
    let s = |v: &serde_json::Value| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
    for (label, v) in [
        ("engine", &c["facts"]["engine_protocol"]),
        ("enrichment", &c["facts"]["enrichment"]["protocol"]),
        ("detector", &c["profile"]["detector_version"]),
        ("ranking", &c["recommendation"]["ranking_version"]),
        ("coaching input", &c["recommendation"]["coaching_input_version"]),
        ("trainer", &c["recommendation"]["trainer_version"]),
        ("focus", &c["recommendation"]["focus_version"]),
        ("models", &c["language"]["models"]),
    ] {
        print(&format!("  {label:<15} {}", s(v)));
    }
    Ok(0)
}

async fn diff(a: &str, b: &str, pool: Option<&PgPool>, print: &dyn Fn(&str)) -> anyhow::Result<i32> {
    let (ma, mb) = (
        load(&source_of(a), pool).await?,
        load(&source_of(b), pool).await?,
    );
    let changes = algo::diff::diff(&ma.hashed(), &mb.hashed());
    print(&format!("{} -> {}", ma.id(), mb.id()));
    if changes.is_empty() {
        print("  no component differs");
        return Ok(0);
    }
    for l in algo::diff::render(&changes) {
        print(&format!("  {l}"));
    }
    Ok(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_name_their_source() {
        assert_eq!(source_of("current"), Source::Current);
        assert_eq!(
            source_of("algo-1.0.0+abcdef012345"),
            Source::Id("algo-1.0.0+abcdef012345".to_owned())
        );
        assert_eq!(source_of("a.json"), Source::File(PathBuf::from("a.json")));
    }
}
