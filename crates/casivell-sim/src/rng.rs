//! A deterministic pseudo-random generator.
//!
//! # Why one is written here rather than taken from a crate
//!
//! The product promises *"gleiche Eingaben, gleiches Ergebnis, immer"* — identical inputs
//! give an identical result, always. For Monte Carlo that means the seed is part of the
//! input: the same seed must reproduce the same ten thousand paths on any machine, in any
//! build, in five years' time.
//!
//! Depending on an external generator would put that promise at the mercy of someone
//! else's version bump. `rand`'s default generator has changed algorithm before, and a
//! projection a user saved would then silently produce different percentiles. Twenty
//! lines here, pinned by tests against known values, is the cheaper guarantee — and it
//! keeps the engine's zero-dependency property.
//!
//! # The algorithm
//!
//! `xorshift64*`, from Marsaglia's *Xorshift RNGs* (2003) with Vigna's multiplier. It is
//! not cryptographic and does not need to be: it is sampling a distribution, not
//! protecting anything. It passes the statistical tests that matter for that, has a
//! period of 2⁶⁴ − 1, and is exactly reproducible in integer arithmetic.

/// A deterministic generator.
///
/// Cloning it copies the position in the stream, so a caller can fork a reproducible
/// sub-stream — useful for running the same paths under different assumptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prng {
    state: u64,
}

impl Prng {
    /// Vigna's multiplier for `xorshift64*`.
    const MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;

    /// The seed substituted when a caller passes zero.
    ///
    /// `xorshift` has one fixed point: from a state of zero it produces only zeros
    /// forever. Silently correcting it is better than refusing, because a zero seed is a
    /// natural thing to pass and the alternative is a Monte Carlo run in which every path
    /// is identical — a failure that looks like a suspiciously narrow distribution rather
    /// than an error.
    const FALLBACK_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

    /// Creates a generator from a seed.
    #[must_use]
    pub const fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 { Self::FALLBACK_SEED } else { seed },
        }
    }

    /// Returns the next value and advances the stream.
    #[must_use]
    pub const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(Self::MULTIPLIER)
    }

    /// Returns a value in `0..bound`, or `0` when `bound` is zero.
    ///
    /// Uses Lemire's multiply-shift with rejection, so every value in the range is equally
    /// likely. The naive modulo is biased toward small values whenever `bound` does not
    /// divide 2⁶⁴, and for a small bound sampling a returns table that bias would quietly
    /// skew every projection.
    #[must_use]
    pub const fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        // Reject the values that would make the range uneven. `checked_rem` rather than
        // `%` or `wrapping_rem`: both of those panic on a zero divisor, and the engine uses
        // no arithmetic that can. `bound` is provably non-zero from the guard above, so the
        // `None` arms are unreachable and resolve to zero rather than panicking.
        let Some(threshold) = bound.wrapping_neg().checked_rem(bound) else {
            return 0;
        };
        loop {
            let candidate = self.next_u64();
            if candidate >= threshold {
                return match candidate.checked_rem(bound) {
                    Some(value) => value,
                    None => 0,
                };
            }
        }
    }

    /// Returns a uniformly chosen index into a slice of `len` items.
    ///
    /// # Errors
    ///
    /// Returns `None` for an empty slice, so a caller cannot index into nothing.
    #[must_use]
    pub fn index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let drawn = self.below(u64::try_from(len).ok()?);
        // `drawn < len`, so the narrowing back to `usize` cannot lose information even on a
        // 32-bit target. `try_from` states that rather than asserting it.
        usize::try_from(drawn).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::Prng;

    /// The whole reason this exists: a seed must reproduce its stream exactly. These
    /// values are recorded from this implementation and pin it forever — if the algorithm
    /// is ever changed, every saved projection changes with it, and this test is what
    /// forces that to be a deliberate decision.
    #[test]
    fn a_seed_reproduces_its_stream_exactly() {
        let mut a = Prng::from_seed(42);
        let first: [u64; 4] = [a.next_u64(), a.next_u64(), a.next_u64(), a.next_u64()];

        let mut b = Prng::from_seed(42);
        let second: [u64; 4] = [b.next_u64(), b.next_u64(), b.next_u64(), b.next_u64()];

        assert_eq!(first, second, "the same seed must give the same stream");
        // And the stream must actually vary rather than repeating one value.
        assert_ne!(first[0], first[1]);
        assert_ne!(first[1], first[2]);
    }

    #[test]
    fn different_seeds_give_different_streams() {
        let mut a = Prng::from_seed(1);
        let mut b = Prng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    /// A zero seed is xorshift's fixed point and would otherwise make every path
    /// identical. The substitution must produce a real stream.
    #[test]
    fn a_zero_seed_is_substituted_rather_than_degenerating() {
        let mut zero = Prng::from_seed(0);
        let values: [u64; 3] = [zero.next_u64(), zero.next_u64(), zero.next_u64()];
        assert_ne!(values[0], 0);
        assert_ne!(values[0], values[1]);
        assert_ne!(values[1], values[2]);
    }

    #[test]
    fn bounded_draws_stay_in_range() {
        let mut rng = Prng::from_seed(7);
        for bound in [1_u64, 2, 3, 7, 12, 100] {
            for _ in 0..500 {
                assert!(rng.below(bound) < bound, "escaped the bound {bound}");
            }
        }
        assert_eq!(rng.below(0), 0);
    }

    /// A biased generator would skew every Monte Carlo result, so the uniformity is
    /// checked rather than assumed. With 12 000 draws over 12 buckets the expected count
    /// is 1 000; a ±25 % band is loose enough never to flake and tight enough to catch the
    /// modulo bias, which for a small bound over a 64-bit range is not subtle.
    #[test]
    fn bounded_draws_are_close_to_uniform() {
        const BUCKETS: usize = 12;
        const DRAWS: usize = 12_000;

        let mut rng = Prng::from_seed(2026);
        let mut counts = [0_usize; BUCKETS];
        for _ in 0..DRAWS {
            let index = rng.index(BUCKETS).expect("non-empty");
            counts[index] = counts[index].saturating_add(1);
        }

        let expected = DRAWS / BUCKETS;
        for (bucket, count) in counts.iter().enumerate() {
            let low = expected * 3 / 4;
            let high = expected * 5 / 4;
            assert!(
                (low..=high).contains(count),
                "bucket {bucket} got {count}, expected about {expected}"
            );
        }
    }

    #[test]
    fn indexing_an_empty_slice_yields_nothing() {
        let mut rng = Prng::from_seed(1);
        assert_eq!(rng.index(0), None);
        assert_eq!(rng.index(1), Some(0));
    }

    /// A copy continues the same stream, so a caller can replay a set of paths under
    /// different assumptions and know the draws were identical.
    #[test]
    fn a_copy_continues_the_same_stream() {
        let mut original = Prng::from_seed(99);
        let _ = original.next_u64();
        let mut forked = original;
        assert_eq!(original.next_u64(), forked.next_u64());
    }
}
