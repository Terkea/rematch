use std::path::PathBuf;

/// True inside the dev container (`IN_DOCKER=1`), where missing services are failures, not skips.
pub fn in_docker() -> bool {
    std::env::var("IN_DOCKER").is_ok_and(|v| v == "1")
}

/// The Stockfish binary from `STOCKFISH_PATH`. Returns None outside Docker when it is unset
/// (the test should skip); panics when it is unset inside Docker.
pub fn stockfish() -> Option<PathBuf> {
    match std::env::var_os("STOCKFISH_PATH").filter(|v| !v.is_empty()) {
        Some(path) => Some(PathBuf::from(path)),
        None if in_docker() => panic!("STOCKFISH_PATH must be set when IN_DOCKER=1"),
        None => None,
    }
}
