//! The engine's deterministic random generator.
//!
//! Our own and small: a heavy dependency is not worth a few bits per tick, and
//! the usual `random` crates do not buy what matters here — the same seed must
//! replay the same sequence of events (issue #35).
//!
//! The generator only breaks ties between equally good options: the order
//! directions are tried in during route search. It sets no game rule — where
//! the original chooses by data, the data is what arrives.

/// splitmix64: two multiplies and three XORs over a 64-bit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..limit`, taken without `%`: the high bits of a
    /// widened product, so there is no modulo bias.
    ///
    /// `limit == 0` gives 0 — calling it is pointless, but there is no reason
    /// to panic either: the value influences nothing.
    pub fn below(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            return 0;
        }
        (((self.next_u64() as u128) * (limit as u128)) >> 64) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_the_same_sequence() {
        let mut first = Rng::new(7);
        let mut second = Rng::new(7);
        for _ in 0..16 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = Rng::new(1234);
        let mut seen = [0u32; 8];
        for _ in 0..800 {
            let value = rng.below(8);
            assert!(value < 8);
            seen[value as usize] += 1;
        }
        for count in seen {
            assert!(count > 0, "value never came up in 800 draws");
        }
        assert_eq!(rng.below(0), 0);
    }
}
