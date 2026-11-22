//! Engine-side enrichment: MultiPV, PV completion, threat probe, Syzygy and difficulty
//! (R2 Tasks 23-25). Versioned by its own protocol ids.

pub mod critical;
pub mod difficulty;
pub mod enrich_game;
pub mod persist;
pub mod protocol;
pub mod reenrich;
pub mod store;
pub mod tablebase;
pub mod types;

/// Installs the process-wide rustls crypto provider (ring), as `platforms::client` does.
///
/// reqwest is built with `rustls-no-provider`, so `reqwest::Client::new()` panics unless a
/// provider is installed. Call this before building any `reqwest::Client` in this crate (for
/// example the Maia provider, Task 24). It is idempotent: a provider that is already installed is
/// kept.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_builds_after_installing_the_provider() {
        super::install_crypto_provider();
        super::install_crypto_provider();
        let _client = reqwest::Client::new();
    }
}
