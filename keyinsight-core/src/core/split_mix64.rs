//! Deterministic seedable RNG so generator and skill-model tests are
//! reproducible, ported bit-for-bit from `Core/SplitMix64.swift` (which
//! itself is the standard SplitMix64).
//!
//! The bounded-sampling helpers replace Swift's stdlib
//! `Int.random(in:using:)` / `Array.randomElement(using:)` /
//! `Double.random(in:using:)` and reproduce the stdlib's exact bit mapping
//! (`stdlib/public/core/Random.swift` and `FloatingPointRandom.swift`), so
//! one seed produces the identical exercise on macOS-Swift, native Rust and
//! WASM.

/// A raw 64-bit generator, mirroring Swift's `RandomNumberGenerator`
/// protocol so generator/skill code can be written against the seam.
pub trait Rng64 {
    fn next_u64(&mut self) -> u64;

    /// Uniform integer in `0..bound` (Swift `Int.random(in: 0..<bound)`,
    /// which is `RandomNumberGenerator.next(upperBound:)` on a 64-bit
    /// `UInt`): Lemire's nearly-divisionless method on the full 64-bit word
    /// with the rejection loop, i.e. `m = next() * bound` as a 128-bit
    /// product, retry while `m.low` falls in the biased tail, return
    /// `m.high`.
    fn next_below_u64(&mut self, bound: u64) -> u64 {
        assert!(bound != 0, "next_below needs a positive bound");
        let mut m = u128::from(self.next_u64()) * u128::from(bound);
        if (m as u64) < bound {
            let t = bound.wrapping_neg() % bound;
            while (m as u64) < t {
                m = u128::from(self.next_u64()) * u128::from(bound);
            }
        }
        (m >> 64) as u64
    }

    /// Uniform index in `0..bound` — `next_below_u64` for the `usize`
    /// bounds the app uses (`Array.randomElement`, `Int.random(in:)`).
    /// Computed in 64 bits so 32-bit WASM draws the same values as native.
    fn next_below(&mut self, bound: usize) -> usize {
        self.next_below_u64(bound as u64) as usize
    }

    /// Uniform float in `0..total` (Swift `Double.random(in: 0..<total)`):
    /// the LOW 53 bits of the word (`next() & (1 << 53 - 1)`) scaled by
    /// `ulpOfOne / 2`, times the span; redrawn in the (theoretically
    /// unreachable) case the product rounds up to `total`.
    fn next_f64_below(&mut self, total: f64) -> f64 {
        const MASK_53: u64 = (1u64 << 53) - 1;
        const HALF_ULP_OF_ONE: f64 = 1.0 / (1u64 << 53) as f64;
        loop {
            let unit = (self.next_u64() & MASK_53) as f64 * HALF_ULP_OF_ONE;
            let value = total * unit;
            if value != total {
                return value;
            }
        }
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

    // Known-answer vectors for the Swift stdlib mappings, produced by this
    // Python model of `RandomNumberGenerator.next(upperBound:)` and
    // `Double.random(in: 0..<total)` driven by SplitMix64:
    //
    //   M64 = (1 << 64) - 1
    //   def next_below(g, ub):            # Random.swift, Lemire + rejection
    //       r = g.next(); m = r * ub
    //       if m & M64 < ub:
    //           t = ((-ub) & M64) % ub
    //           while m & M64 < t:
    //               r = g.next(); m = r * ub
    //       return m >> 64
    //   def next_f64(g, total):           # FloatingPointRandom.swift
    //       while True:
    //           v = total * ((g.next() & ((1 << 53) - 1)) * 2.0 ** -53)
    //           if v != total: return v
    //
    //   seed 0,  bounds 7,7,10,3,1,88      -> [6, 3, 0, 2, 0, 28]
    //   seed 42, bounds 10,3,5,6,12,1000   -> [7, 0, 1, 2, 0, 868]
    //   seed 0,  totals 1.0, 3.5, 0.8      -> [0.020535221540219584,
    //                                          2.6926828437200596,
    //                                          0.10909137731226358]
    //   seed 42, totals 1.0, 1.0           -> [0.7248717246943267,
    //                                          0.49648461193240256]
    //   seed 0,  bound 2^63 + 1            -> 243808509735772839 (2 rejections)
    //   seed 6,  bound 3 << 62             -> 6174776236951037874 (1 rejection)
    #[test]
    fn next_below_matches_swift_next_upper_bound() {
        let mut rng = SplitMix64::new(0);
        let drawn: Vec<usize> = [7, 7, 10, 3, 1, 88]
            .iter()
            .map(|&b| rng.next_below(b))
            .collect();
        assert_eq!(drawn, [6, 3, 0, 2, 0, 28]);

        let mut rng = SplitMix64::new(42);
        let drawn: Vec<usize> = [10, 3, 5, 6, 12, 1000]
            .iter()
            .map(|&b| rng.next_below(b))
            .collect();
        assert_eq!(drawn, [7, 0, 1, 2, 0, 868]);
    }

    #[test]
    fn next_below_takes_rejection_path_like_swift() {
        // bound 2^63 + 1: t = 2^63 - 1, so the first two words are rejected.
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_below_u64((1u64 << 63) + 1), 243_808_509_735_772_839);
        // The rejection consumed three words in total.
        let mut plain = SplitMix64::new(0);
        for _ in 0..3 {
            plain.next_u64();
        }
        assert_eq!(rng.next_u64(), plain.next_u64());

        // bound 3 << 62: t = 2^62; seed 6 rejects exactly one word.
        let mut rng = SplitMix64::new(6);
        assert_eq!(rng.next_below_u64(3u64 << 62), 6_174_776_236_951_037_874);
        let mut plain = SplitMix64::new(6);
        for _ in 0..2 {
            plain.next_u64();
        }
        assert_eq!(rng.next_u64(), plain.next_u64());
    }

    #[test]
    fn next_below_bound_one_is_zero_and_consumes_one_word() {
        let mut rng = SplitMix64::new(99);
        for _ in 0..10 {
            assert_eq!(rng.next_below(1), 0);
        }
        let mut plain = SplitMix64::new(99);
        for _ in 0..10 {
            plain.next_u64();
        }
        assert_eq!(rng.next_u64(), plain.next_u64());
    }

    #[test]
    fn next_f64_below_matches_swift_double_random() {
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_f64_below(1.0), 0.020535221540219584);
        assert_eq!(rng.next_f64_below(3.5), 2.6926828437200596);
        assert_eq!(rng.next_f64_below(0.8), 0.10909137731226358);

        let mut rng = SplitMix64::new(42);
        assert_eq!(rng.next_f64_below(1.0), 0.7248717246943267);
        assert_eq!(rng.next_f64_below(1.0), 0.49648461193240256);
    }
}
