//! `worker --version` prints the product version without touching the environment, so the
//! release smoke test (ADR 0098) can run it in the image with no database or Stockfish.

use std::process::Command;

#[test]
fn version_flag_prints_the_product_version_and_needs_no_environment() {
    for flag in ["--version", "-V"] {
        let out = Command::new(env!("CARGO_BIN_EXE_worker"))
            .arg(flag)
            .env_clear()
            .output()
            .expect("running the worker binary");
        assert!(out.status.success(), "{flag}: {out:?}");
        assert_eq!(
            String::from_utf8(out.stdout).expect("utf-8"),
            format!("worker {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
    }
}

#[test]
fn an_unknown_argument_is_refused_before_anything_starts() {
    let out = Command::new(env!("CARGO_BIN_EXE_worker"))
        .arg("--nope")
        .env_clear()
        .output()
        .expect("running the worker binary");
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert!(err.contains("--nope"), "{err}");
}
