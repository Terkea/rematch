# 0021. TypeScript everywhere; Rust only for a profiled hot job

Date: 2026-11-21
Status: Accepted

## Context

Chess logic is CPU-heavy, which raises the question of writing it in Rust. Rust chess libraries (shakmaty, pgn-reader) are strong, but they are GPL-licensed, which would constrain how the product can be distributed.

## Decision

Write all application and chess code in TypeScript. Consider Rust only for a specific batch job that profiling shows to be a bottleneck, and check the licence before using any chess crate.

## Alternatives considered

A Rust core compiled to WASM or native; a Python core.

## Consequences

One language and one toolchain, with shared types end to end. Pure-JS chess code is slower than Rust, which we accept until a profile says otherwise. The Rust option stays open for isolated batch work.

See ADR 0001.
