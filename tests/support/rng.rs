//! Single seeded deterministic byte generator for parser tests (SplitMix64).
//!
//! Included by parser test files via `#[path = "..."] mod rng;` at the top level.

/// Deterministic SplitMix64 generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns the next 64-bit value from the SplitMix64 sequence.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Generates `len` deterministic bytes from successive `next_u64` words (little-endian).
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let word = self.next_u64().to_le_bytes();
            let remaining = len.saturating_sub(out.len());
            out.extend(word.into_iter().take(remaining));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn seed_zero_matches_published_reference() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
        assert_eq!(rng.bytes(5).len(), 5);
    }
}
