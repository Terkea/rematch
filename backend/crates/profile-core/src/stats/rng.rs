//! Seeded mulberry32 RNG (R2 Task 11).
//!
//! Bit-identical to the canonical JavaScript mulberry32 that the TS reference (`createRng`)
//! names: `a = a + 0x6D2B79F5 | 0; t = Math.imul(a ^ a >>> 15, 1 | a);
//! t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 2^32`. The same
//! seed gives the same `u32` stream and therefore the same `f64` stream. `profile-core` never
//! uses an unseeded generator: every stochastic function takes a seed or an `Rng`.

/// mulberry32: 32-bit state, period 2^32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// A generator seeded with `seed` (JS `createRng(seed)` for `0 <= seed < 2^32`).
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The next 32-bit output.
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let a = self.state;
        let t = (a ^ (a >> 15)).wrapping_mul(a | 1);
        let t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61)) ^ t;
        t ^ (t >> 14)
    }

    /// The next uniform draw in `[0, 1)`: `next_u32() / 2^32`, exactly as the JS reference.
    pub fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / 4_294_967_296.0
    }
}
