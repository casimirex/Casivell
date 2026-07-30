//! The growth rates a projection rests on.
//!
//! Two rates, deliberately. More would look more sophisticated without being more
//! defensible: every additional knob is another number a user cannot check, and the
//! honest position is that a forty-year forecast is dominated by whether prices and
//! wages grow, not by the fifth decimal of how they interact.
//!
//! # These are inputs, never hidden constants
//!
//! The defaults below exist so a caller is not forced to invent figures, not because
//! they are correct. Any UI must show them and let a user change them: a projection
//! whose assumptions are buried is a projection presented as a prediction.

use casivell_core::{MoneyError, Rate};

/// Annual growth assumptions for a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assumptions {
    /// Annual price inflation, applied to the tariff Eckwerte and the Soli Freigrenze.
    ///
    /// These track the Existenzminimum and are adjusted for inflation rather than for
    /// wages — the *Tarif auf Rädern* principle, which the Steuerfortentwicklungsgesetz
    /// applies. Under-indexing them is what produces *kalte Progression*, so an
    /// assumption of zero here is a substantive claim, not a neutral one.
    price_inflation: Rate,

    /// Annual wage growth, applied to the contribution ceilings, the
    /// Durchschnittsentgelt, the Bezugsgröße and the Rentenwert.
    ///
    /// These are all derived from average earnings, so they move with wages and not
    /// with prices. Historically the two series differ by more than a point, which is
    /// exactly why one rate for both would be wrong.
    wage_growth: Rate,
}

impl Assumptions {
    /// The widest annual rate accepted in either direction: ±20 %.
    ///
    /// Not a prediction about plausibility so much as a bound that makes the
    /// compounding in [`crate::growth`] provably safe over a century (JPL R2), and
    /// that catches a percentage supplied where a rate was meant.
    pub const MAX_ABS_PERCENT_MILLIS: i64 = 20_000;

    /// A default price inflation of 2.0 %.
    ///
    /// The ECB's medium-term target. Chosen because it is the figure the institution
    /// responsible for it has publicly committed to, which makes it defensible to
    /// state — not because inflation will be 2.0 %.
    pub const DEFAULT_PRICE_INFLATION_PERCENT_MILLIS: i64 = 2_000;

    /// A default wage growth of 2.8 %.
    ///
    /// Roughly the long-run German average: price inflation plus something under a
    /// point of real growth. Deliberately above the price assumption, because a
    /// projection in which wages track prices exactly would understate both future
    /// contribution ceilings and future pension entitlements.
    pub const DEFAULT_WAGE_GROWTH_PERCENT_MILLIS: i64 = 2_800;

    /// Constructs a set of assumptions.
    ///
    /// # Errors
    ///
    /// [`MoneyError::RateOutOfDomain`] if either rate exceeds
    /// [`Self::MAX_ABS_PERCENT_MILLIS`].
    pub const fn new(price_inflation: Rate, wage_growth: Rate) -> Result<Self, MoneyError> {
        let bound = match Rate::from_percent_millis(Self::MAX_ABS_PERCENT_MILLIS) {
            Ok(r) => r.ppm(),
            Err(e) => return Err(e),
        };
        let Some(lower) = bound.checked_neg() else {
            return Err(MoneyError::Overflow);
        };
        if price_inflation.ppm() > bound || price_inflation.ppm() < lower {
            return Err(MoneyError::RateOutOfDomain {
                ppm: price_inflation.ppm(),
            });
        }
        if wage_growth.ppm() > bound || wage_growth.ppm() < lower {
            return Err(MoneyError::RateOutOfDomain {
                ppm: wage_growth.ppm(),
            });
        }
        Ok(Self {
            price_inflation,
            wage_growth,
        })
    }

    /// Constructs assumptions from thousandths of a percent, as a caller reading a
    /// figure off a page would write them.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub const fn from_percent_millis(
        price_inflation: i64,
        wage_growth: i64,
    ) -> Result<Self, MoneyError> {
        let price = match Rate::from_percent_millis(price_inflation) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        let wage = match Rate::from_percent_millis(wage_growth) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        Self::new(price, wage)
    }

    /// Assumptions in which nothing grows.
    ///
    /// Useful for isolating the effect of a decision from the effect of inflation, and
    /// for tests that want projection to be the identity. It is not a neutral
    /// assumption in substance — zero indexation of the Eckwerte means real tax rises
    /// every year — which is the point of being able to select it deliberately.
    #[must_use]
    pub const fn frozen() -> Self {
        Self {
            price_inflation: Rate::ZERO,
            wage_growth: Rate::ZERO,
        }
    }

    /// Annual price inflation.
    #[must_use]
    pub const fn price_inflation(&self) -> Rate {
        self.price_inflation
    }

    /// Annual wage growth.
    #[must_use]
    pub const fn wage_growth(&self) -> Rate {
        self.wage_growth
    }
}

impl Default for Assumptions {
    /// The documented defaults: 2.0 % prices, 2.8 % wages.
    fn default() -> Self {
        // The constants are within the bound by construction, so the fallback is
        // unreachable; it exists because `Default::default` cannot fail and panicking
        // is denied. `the_defaults_are_within_the_domain` proves it is never taken.
        Self::from_percent_millis(
            Self::DEFAULT_PRICE_INFLATION_PERCENT_MILLIS,
            Self::DEFAULT_WAGE_GROWTH_PERCENT_MILLIS,
        )
        .unwrap_or_else(|_| Self::frozen())
    }
}

#[cfg(test)]
mod tests {
    use super::Assumptions;
    use casivell_core::{MoneyError, Rate};

    /// `Default` cannot fail, so it falls back to frozen assumptions. That fallback
    /// must never actually be taken, or every default projection would silently stop
    /// indexing anything.
    #[test]
    fn the_defaults_are_within_the_domain() {
        let defaults = Assumptions::default();
        assert_eq!(
            defaults.price_inflation(),
            Rate::from_percent_millis(2_000).expect("valid")
        );
        assert_eq!(
            defaults.wage_growth(),
            Rate::from_percent_millis(2_800).expect("valid")
        );
        assert_ne!(
            defaults,
            Assumptions::frozen(),
            "the Default fallback was taken, so the constants are out of domain"
        );
    }

    /// Wages are assumed to outpace prices. A default in which they did not would
    /// understate future ceilings and pensions, so the relationship is asserted rather
    /// than left to whoever next edits the constants.
    #[test]
    fn wages_are_assumed_to_outpace_prices() {
        let d = Assumptions::default();
        assert!(d.wage_growth().ppm() > d.price_inflation().ppm());
    }

    #[test]
    fn frozen_assumptions_grow_nothing() {
        let frozen = Assumptions::frozen();
        assert!(frozen.price_inflation().is_zero());
        assert!(frozen.wage_growth().is_zero());
    }

    #[test]
    fn implausible_rates_are_refused() {
        let over = Assumptions::MAX_ABS_PERCENT_MILLIS.saturating_add(1);
        assert!(matches!(
            Assumptions::from_percent_millis(over, 2_800),
            Err(MoneyError::RateOutOfDomain { .. })
        ));
        assert!(matches!(
            Assumptions::from_percent_millis(2_000, -over),
            Err(MoneyError::RateOutOfDomain { .. })
        ));
        // The bound itself is accepted, in both directions.
        assert!(
            Assumptions::from_percent_millis(
                Assumptions::MAX_ABS_PERCENT_MILLIS,
                -Assumptions::MAX_ABS_PERCENT_MILLIS
            )
            .is_ok()
        );
    }

    /// Deflation is representable: a projection must be able to model it, even though
    /// it is not the default.
    #[test]
    fn negative_growth_is_representable() {
        let deflationary =
            Assumptions::from_percent_millis(-500, -1_000).expect("within the domain");
        assert!(deflationary.price_inflation().ppm() < 0);
        assert!(deflationary.wage_growth().ppm() < 0);
    }
}
