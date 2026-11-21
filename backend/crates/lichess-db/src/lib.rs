//! Lichess database pipeline (operator CLI, R-11): streamed download, SHA-256, zstd, split,
//! headers, sampling, parallel parse, explorer build, reference selection and Syzygy fetch.

pub mod bench;
pub mod config;
pub mod download;
pub mod explorer_build;
pub mod headers;
pub mod movetext;
pub mod pipeline;
pub mod reference_select;
pub mod sample;
pub mod split;
pub mod syzygy;
