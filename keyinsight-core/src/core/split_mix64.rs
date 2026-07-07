//! Deterministic seedable RNG so generator and skill-model tests are
//! reproducible, ported bit-for-bit from `Core/SplitMix64.swift` (which
//! itself is the standard SplitMix64).
//!
//! The bounded-sampling helpers replace Swift's stdlib
//! `Int.random(in:using:)` / `Double.random(in:using:)`. Swift's exact
//! bit-mapping is stdlib-private and not part of the app being ported; what
//! the app requires (and its tests pin) is that one seed produces one
//! exercise on every platform — which these documented mappings guarantee
//! across native and WASM builds of this port.

/// A raw 64-bit generator, mirroring Swift's `RandomNumberGenerator`
/// protocol so generator/skill code can be written against the seam.
pub trait Rng64 {
    fn next_u64(&mut self) -> u64;

    /// Uniform integer in `0..bound` (Swift `Int.random(in: 0..<bound)`).
    /// Rejection-free multiply-shift mapping (Lemire without the rejection
    /// loop — bias is < 2⁻³² for the tiny bounds this app uses).
    fn next_below(&mut self, bound: usize) -> usize {
        debug_assert!(bound > 0, "next_below needs a positive bound");
        (((self.next_u64() >> 32) * (bound as u64)) >> 32) as usize
    }

    /// Uniform float in `0..total` (Swift `Double.random(in: 0..<total)`).
    /// 53-bit mantissa mapping.
    fn next_f64_below(&mut self, total: f64) -> f64 {
        let unit = (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
        unit * total
    }
}

/// SplitMix64, matching the Swift source exactly.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Rng64 for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values computed from the canonical SplitMix64 algorithm
    /// (identical to the Swift implementation) for seed 0 and seed 42 —
    /// this pins the bit-for-bit port.
    #[test]
    fn matches_canonical_splitmix64_stream() {
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);

        let mut rng = SplitMix64::new(42);
        assert_eq!(rng.next_u64(), 0xBDD7_3226_2FEB_6E95);
    }

    #[test]
    fn same_seed_same_stream() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn bounded_sampling_stays_in_range() {
        let mut rng = SplitMix64::new(1);
        for _ in 0..1000 {
            assert!(rng.next_below(7) < 7);
            let f = rng.next_f64_below(3.5);
            assert!((0.0..3.5).contains(&f));
        }
    }
}
