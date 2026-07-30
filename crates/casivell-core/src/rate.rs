//! [`Rate`]: a proportion stored as an integer number of parts per million.
//!
//! # Why parts per million
//!
//! Every rate German social and tax law actually uses is expressible exactly in
//! ppm. The contribution rates are quoted to at most three decimal places of a
//! percent — 14.6 %, 2.9 %, 3.6 %, 18.6 %, 5.5 %, 0.6 % — and a percent is
//! 10 000 ppm, so a thousandth of a percent is 10 ppm. There is no statutory rate
//! that ppm cannot hold, and no need for a general rational type.
//!
//! Rates are constructed from *percent-millis* (thousandths of a percent) rather
//! than from ppm directly. Legislation and press releases speak in percent, so
//! the constructor takes the unit the source document uses: `14.6 %` is written
//! `Rate::from_percent_millis(14_600)`. Transcribing a figure should not require
//! the reader to multiply anything in their head, because that is exactly where
//! transcription errors enter.

use crate::money::MoneyError;

/// A proportion, as an integer number of parts per million.
///
/// `Rate::ONE` is 100 %. Negative rates are permitted so the type can also carry
/// a deflationary price index, but the plausibility band is deliberately narrow
/// so that a percent value passed where ppm was expected is rejected rather than
/// silently applied as a factor ten thousand times too small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Rate {
    ppm: i64,
}

impl Rate {
    /// Parts per million in one whole, i.e. 100 %.
    pub const ONE: Self = Self { ppm: 1_000_000 };

    /// Zero percent.
    pub const ZERO: Self = Self { ppm: 0 };

    /// Parts per million in one percent.
    pub const PPM_PER_PERCENT: i64 = 10_000;

    /// Parts per million in one thousandth of a percent.
    pub const PPM_PER_PERCENT_MILLI: i64 = 10;

    /// The widest rate representable: ±1 000 %.
    ///
    /// A ten-fold factor is far beyond any statutory rate while still admitting
    /// cumulative growth factors. The bound is what makes the `i128`
    /// intermediate in [`crate::Money::mul_rate`] provably sufficient.
    pub const MAX_ABS_PPM: i64 = 10_000_000;

    /// Constructs a rate from thousandths of a percent.
    ///
    /// `from_percent_millis(14_600)` is 14.6 %.
    ///
    /// # Errors
    ///
    /// [`MoneyError::RateOutOfDomain`] if the rate exceeds
    /// [`Self::MAX_ABS_PPM`], which in practice means a unit mix-up.
    pub const fn from_percent_millis(percent_millis: i64) -> Result<Self, MoneyError> {
        match percent_millis.checked_mul(Self::PPM_PER_PERCENT_MILLI) {
            Some(ppm) => Self::from_ppm(ppm),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Constructs a rate from whole percent.
    ///
    /// # Errors
    ///
    /// As [`Self::from_percent_millis`].
    pub const fn from_percent(percent: i64) -> Result<Self, MoneyError> {
        match percent.checked_mul(Self::PPM_PER_PERCENT) {
            Some(ppm) => Self::from_ppm(ppm),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Constructs a rate from parts per million.
    ///
    /// Prefer [`Self::from_percent_millis`] when transcribing from a statute.
    ///
    /// # Errors
    ///
    /// [`MoneyError::RateOutOfDomain`] if outside [`Self::MAX_ABS_PPM`].
    pub const fn from_ppm(ppm: i64) -> Result<Self, MoneyError> {
        if ppm > Self::MAX_ABS_PPM || ppm < -Self::MAX_ABS_PPM {
            return Err(MoneyError::RateOutOfDomain { ppm });
        }
        Ok(Self { ppm })
    }

    /// Returns the rate in parts per million.
    #[must_use]
    pub const fn ppm(self) -> i64 {
        self.ppm
    }

    /// Adds two rates.
    ///
    /// This is how a composite contribution rate is assembled — the general GKV
    /// rate plus a fund's supplementary rate, for instance — and it is a distinct
    /// operation from applying two rates in sequence.
    ///
    /// # Errors
    ///
    /// [`MoneyError::RateOutOfDomain`] if the sum leaves the plausible band.
    pub const fn add(self, other: Self) -> Result<Self, MoneyError> {
        match self.ppm.checked_add(other.ppm) {
            Some(sum) => Self::from_ppm(sum),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Subtracts `other` from `self`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::RateOutOfDomain`] if the difference leaves the plausible band.
    pub const fn sub(self, other: Self) -> Result<Self, MoneyError> {
        match self.ppm.checked_sub(other.ppm) {
            Some(diff) => Self::from_ppm(diff),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Halves the rate, rounding toward zero.
    ///
    /// Social-insurance contributions are shared equally between employer and
    /// employee. Halving the *rate* and halving the resulting *amount* are not
    /// the same operation once rounding enters, and the statutes split the
    /// contribution, not the rate — so this exists for the cases that genuinely
    /// need a half rate, and callers splitting a contribution should use
    /// [`crate::Money::div_int`] instead.
    ///
    /// # Errors
    ///
    /// [`MoneyError::DivisionByZero`] cannot occur; the signature is uniform with
    /// the rest of the API.
    pub const fn half(self) -> Result<Self, MoneyError> {
        Self::from_ppm(self.ppm / 2)
    }

    /// Returns whether the rate is exactly zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.ppm == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Rate;
    use crate::money::MoneyError;

    #[test]
    fn percent_millis_is_the_transcription_unit() {
        // 14.6 % — the statutory general GKV rate, § 241 SGB V.
        let gkv = Rate::from_percent_millis(14_600);
        assert_eq!(gkv.map(Rate::ppm), Ok(146_000));
        // 5.5 % — the Solidaritätszuschlag rate, § 4 SolzG 1995.
        assert_eq!(Rate::from_percent_millis(5_500).map(Rate::ppm), Ok(55_000));
        // 0.6 % — the childless surcharge, § 55 Abs. 3 SGB XI.
        assert_eq!(Rate::from_percent_millis(600).map(Rate::ppm), Ok(6_000));
    }

    #[test]
    fn whole_percent_and_percent_millis_agree() {
        assert_eq!(Rate::from_percent(42), Rate::from_percent_millis(42_000));
        assert_eq!(Rate::from_percent(100), Ok(Rate::ONE));
    }

    /// The plausibility band's real job: catching a percent figure handed to the
    /// ppm constructor, or vice versa.
    #[test]
    fn rejects_implausible_rates() {
        assert!(matches!(
            Rate::from_ppm(Rate::MAX_ABS_PPM + 1),
            Err(MoneyError::RateOutOfDomain { .. })
        ));
        // 14 600 % — what happens if percent-millis is mistaken for percent.
        assert!(matches!(
            Rate::from_percent(14_600),
            Err(MoneyError::RateOutOfDomain { .. })
        ));
    }

    #[test]
    fn composite_rates_add() {
        // General rate 14.6 % plus the 2026 average supplementary rate 2.9 %.
        let general = Rate::from_percent_millis(14_600).expect("in domain");
        let supplementary = Rate::from_percent_millis(2_900).expect("in domain");
        let total = general.add(supplementary).expect("in domain");
        assert_eq!(total, Rate::from_percent_millis(17_500).expect("in domain"));
    }

    #[test]
    fn negative_rates_are_representable_for_price_indices() {
        let deflation = Rate::from_percent_millis(-500).expect("in domain");
        assert_eq!(deflation.ppm(), -5_000);
    }

    #[test]
    fn halving_is_exact_for_the_statutory_rates() {
        let pension = Rate::from_percent_millis(18_600).expect("in domain");
        assert_eq!(
            pension.half(),
            Rate::from_percent_millis(9_300).map_err(|_| MoneyError::Overflow)
        );
    }
}
