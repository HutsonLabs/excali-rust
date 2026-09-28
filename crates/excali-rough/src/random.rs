//! rough.js's random source (`bin/math.js`).
//!
//! ```js
//! next() {
//!   if (this.seed) {
//!     return ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31;
//!   } else {
//!     return Math.random();
//!   }
//! }
//! ```
//!
//! A Park–Miller style LCG with multiplier 48271 whose state is the 32-bit
//! product (`Math.imul` wraps); only a seed of 0 falls back to
//! `Math.random`. Excalidraw stores a non-zero seed per element, so every
//! drawing it makes is reproducible; the fallback exists for callers that
//! leave `seed` at its default of 0.

use std::cell::Cell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// `Random` from rough.js `bin/math.js`.
///
/// rough.js creates an options object's randomizer on its first draw
/// (`renderer.js` `random(ops)`), and one place tells the difference: the
/// pattern fillers draw their skip offset from `o.randomizer?.next()`, so
/// before any draw they fall back to `Math.random` without creating it.
/// [`Random::has_drawn`] is that "the randomizer exists" test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Random {
    seed: i32,
    drawn: bool,
}

impl Random {
    /// `new Random(seed)`.
    pub fn new(seed: i32) -> Self {
        Self { seed, drawn: false }
    }

    /// Whether [`Random::next`] has been called: in rough.js, whether the
    /// options object holding this sequence has a `randomizer` yet.
    pub fn has_drawn(&self) -> bool {
        self.drawn
    }

    /// `next()`: the next number in `[0, 1)`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        self.drawn = true;
        if self.seed != 0 {
            self.seed = 48271i32.wrapping_mul(self.seed);
            f64::from(self.seed & 0x7fff_ffff) / 2_147_483_648.0
        } else {
            math_random()
        }
    }
}

/// `randomSeed()` (exposed as `RoughGenerator.newSeed()`):
/// `Math.floor(Math.random() * 2 ** 31)`.
pub fn random_seed() -> i32 {
    // math_random() < 1, so the product is below 2^31 and fits.
    (math_random() * 2_147_483_648.0).floor() as i32
}

thread_local! {
    static STATE: Cell<(u64, u64)> = Cell::new(initial_state());
}

/// Seeds the fallback generator from std's per-process hash keys (the OS
/// random source where std has one).
fn initial_state() -> (u64, u64) {
    let keys = RandomState::new();
    let mut a = keys.build_hasher();
    a.write_u64(0x9e37_79b9_7f4a_7c15);
    let mut b = keys.build_hasher();
    b.write_u64(0xbf58_476d_1ce4_e5b9);
    let (s0, s1) = (a.finish(), b.finish());
    // xorshift128+ must not start from all zeros
    if s0 == 0 && s1 == 0 {
        (1, 2)
    } else {
        (s0, s1)
    }
}

/// `Math.random()`: a double in `[0, 1)` from xorshift128+, the generator V8
/// uses for `Math.random` (its sequence is not reproducible in JavaScript
/// either, so only the distribution matters).
pub(crate) fn math_random() -> f64 {
    STATE.with(|state| {
        let (mut s1, s0) = state.get();
        let result = s0.wrapping_add(s1);
        s1 ^= s1 << 23;
        state.set((s0, s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5)));
        // the top 53 bits as a double in [0, 1)
        (result >> 11) as f64 / (1u64 << 53) as f64
    })
}
