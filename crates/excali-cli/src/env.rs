//! What restore asks of its environment: the time, fresh ids and random
//! integers (`getUpdatedTimestamp`, `randomId`, `randomInteger`,
//! `packages/common/src/utils.ts:552`, `random.ts:9-16`).
//!
//! Upstream uses `Date.now()`, nanoid and roughjs' random. The CLI uses
//! the clock and a splitmix64 generator seeded from it, except when
//! `SOURCE_DATE_EPOCH` is set (the reproducible-builds convention): then
//! the time is that many seconds and the generator is seeded from it, so
//! the same input writes the same bytes.

use std::time::{SystemTime, UNIX_EPOCH};

use excali_core::restore::RestoreEnv;
use excali_scene::sticky_note::Clock;

/// nanoid's alphabet (`nanoid/url-alphabet`), which `randomId` draws from.
const URL_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

/// The CLI's restore environment.
#[derive(Clone, Debug)]
pub struct CliEnv {
    now: f64,
    state: u64,
}

impl CliEnv {
    /// A fixed time (epoch milliseconds) and seed.
    pub fn new(now: f64, seed: u64) -> CliEnv {
        CliEnv { now, state: seed }
    }

    /// From `SOURCE_DATE_EPOCH` when it holds a number of seconds,
    /// otherwise from the clock.
    pub fn from_environment() -> CliEnv {
        let fixed = std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());
        match fixed {
            Some(secs) => CliEnv::new(secs as f64 * 1000.0, secs),
            None => {
                let since = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                let seed = since.as_nanos() as u64 ^ u64::from(std::process::id()).rotate_left(32);
                CliEnv::new(since.as_millis() as f64, seed)
            }
        }
    }

    /// splitmix64.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl RestoreEnv for CliEnv {
    fn now(&mut self) -> f64 {
        self.now
    }

    /// 21 characters of nanoid's alphabet, as `nanoid()` gives.
    fn random_id(&mut self) -> String {
        (0..21)
            .map(|_| char::from(URL_ALPHABET[(self.next() >> 58) as usize]))
            .collect()
    }

    /// An integer in `0..2^31`.
    fn random_integer(&mut self) -> f64 {
        (self.next() >> 33) as f64
    }
}

/// What a sticky note's date footer reads: the time [`CliEnv`] would give
/// (`SOURCE_DATE_EPOCH` or the clock), in UTC, since the CLI does not read
/// the host's time zone.
pub fn render_clock() -> Clock {
    Clock::utc(CliEnv::from_environment().now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_nanoid_shaped_and_distinct() {
        let mut env = CliEnv::new(0.0, 7);
        let a = env.random_id();
        let b = env.random_id();
        assert_eq!(a.len(), 21);
        assert!(a.bytes().all(|c| URL_ALPHABET.contains(&c)));
        assert_ne!(a, b);
    }

    #[test]
    fn integers_are_below_two_to_the_31() {
        let mut env = CliEnv::new(0.0, 1);
        for _ in 0..1000 {
            let n = env.random_integer();
            assert!((0.0..2_147_483_648.0).contains(&n) && n.fract() == 0.0);
        }
    }

    #[test]
    fn a_seed_gives_the_same_sequence() {
        let mut a = CliEnv::new(5.0, 42);
        let mut b = CliEnv::new(5.0, 42);
        assert_eq!(a.random_id(), b.random_id());
        assert_eq!(a.random_integer(), b.random_integer());
        assert_eq!(a.now(), 5.0);
    }
}
