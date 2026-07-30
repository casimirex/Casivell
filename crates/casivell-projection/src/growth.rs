//! Compounding a monetary amount forward, one year at a time.
//!
//! # Why year by year rather than by a power
//!
//! Statutory indexation is annual: each year's figure is derived from the previous
//! year's and published as a rounded amount, which then becomes the base for the next.
//! Applying `(1 + g)^n` in one step would compound the *unrounded* value and drift away
//! from the sequence a legislature would actually produce.
//!
//! So these functions iterate, rounding at each step exactly where the statute rounds.
//! The loop is bounded by [`MAX_STEPS`] — the century `casivell_core::TaxYear`
//! represents — which satisfies JPL R2, and the whole computation stays in integers.
//!
//! # Statutory rounding granularity
//!
//! Contribution ceilings are not rounded to the euro. § 159 SGB VI rounds the annual
//! pension ceiling to a multiple of 600 € — so the monthly figure is a multiple of
//! 50 € — and § 6 Abs. 7 SGB V rounds the health ceiling to a multiple of 450 €, for a
//! monthly multiple of 37.50 €. Both are verifiable against the enacted figures:
//! 101 400 = 169 × 600 and 69 750 = 155 × 450.
//!
//! Reproducing that granularity is not cosmetic. A projected ceiling of 104 271 €
//! would be obviously invented; 104 400 € is the shape of a real statutory figure, and
//! rounding to the grid is also what keeps the monthly amount an exact number of cents.

use casivell_core::{Money, MoneyError, Rate, Rounding};

/// The most years a projection will compound over.
///
/// One more than the span the year type represents, so the bound can never be the
/// binding constraint on a legal projection while still bounding the loop.
pub const MAX_STEPS: u32 = 101;

/// Compounds `base` forward `steps` years, rounding to whole euro each year.
///
/// For figures the statute states in whole euro: the tariff Eckwerte, the
/// Durchschnittsentgelt, the Soli Freigrenze.
///
/// # Errors
///
/// [`MoneyError::Overflow`] if the amount leaves the representable domain, or if
/// `steps` exceeds [`MAX_STEPS`].
pub fn compound_to_euro(base: Money, rate: Rate, steps: u32) -> Result<Money, MoneyError> {
    // Nearest euro, not truncated: an indexation that floored every year would lose
    // up to a euro annually and drift measurably downward over a forty-year horizon.
    compound(base, rate, steps, |amount| snap_to_multiple(amount, 1))
}

/// Compounds `base` forward `steps` years, rounding to whole cent each year.
///
/// For figures stated to the cent, of which the Rentenwert is the one that matters.
///
/// # Errors
///
/// As [`compound_to_euro`].
pub fn compound_to_cent(base: Money, rate: Rate, steps: u32) -> Result<Money, MoneyError> {
    // `Money` is already an integer number of cents, so a rate application rounded to
    // the cent is the identity on the representation; the rounding happens inside
    // `mul_rate`.
    compound(base, rate, steps, Ok)
}

/// Compounds `base` forward `steps` years, snapping to the nearest multiple of
/// `multiple_euro` each year.
///
/// For the contribution ceilings. See the module documentation for the statutory
/// granularities and why they are worth reproducing.
///
/// # Errors
///
/// As [`compound_to_euro`], plus [`MoneyError::DivisionByZero`] if `multiple_euro` is
/// zero.
pub fn compound_to_multiple(
    base: Money,
    rate: Rate,
    steps: u32,
    multiple_euro: i64,
) -> Result<Money, MoneyError> {
    if multiple_euro <= 0 {
        return Err(MoneyError::DivisionByZero);
    }
    compound(base, rate, steps, |amount| {
        snap_to_multiple(amount, multiple_euro)
    })
}

/// Rounds an amount to the nearest multiple of `multiple_euro` euro.
///
/// # Errors
///
/// [`MoneyError::DivisionByZero`] if `multiple_euro` is zero; [`MoneyError::Overflow`]
/// on a non-representable result.
pub fn snap_to_multiple(amount: Money, multiple_euro: i64) -> Result<Money, MoneyError> {
    if multiple_euro <= 0 {
        return Err(MoneyError::DivisionByZero);
    }
    let grid_cents = multiple_euro
        .checked_mul(Money::CENTS_PER_EURO)
        .ok_or(MoneyError::Overflow)?;
    // Divide onto the grid with commercial rounding, then multiply back.
    let steps_on_grid = casivell_core::div_round_half_up(amount.cents(), grid_cents)?;
    let snapped = steps_on_grid
        .checked_mul(grid_cents)
        .ok_or(MoneyError::Overflow)?;
    Money::from_cents(snapped)
}

/// The shared compounding loop.
///
/// `round` is applied after each year's growth, so the rounded figure is the base for
/// the next year — which is how statutory indexation actually behaves.
fn compound<F>(base: Money, rate: Rate, steps: u32, round: F) -> Result<Money, MoneyError>
where
    F: Fn(Money) -> Result<Money, MoneyError>,
{
    if steps > MAX_STEPS {
        return Err(MoneyError::Overflow);
    }
    let factor = Rate::ONE.add(rate)?;
    let mut amount = base;
    for _ in 0..steps {
        // Rounding half up rather than truncating: an indexation that truncated every
        // year would drift systematically downward over four decades.
        amount = round(amount.mul_rate(factor, Rounding::HalfUp)?)?;
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_STEPS, compound_to_cent, compound_to_euro, compound_to_multiple, snap_to_multiple,
    };
    use casivell_core::{Money, MoneyError, Rate};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).expect("in domain")
    }

    fn rate(percent_millis: i64) -> Rate {
        Rate::from_percent_millis(percent_millis).expect("valid")
    }

    #[test]
    fn zero_steps_is_the_identity() {
        let base = euro(12_348);
        assert_eq!(compound_to_euro(base, rate(2_000), 0).unwrap(), base);
        assert_eq!(compound_to_cent(base, rate(2_000), 0).unwrap(), base);
        assert_eq!(
            compound_to_multiple(base, rate(2_000), 0, 600).unwrap(),
            base
        );
    }

    #[test]
    fn zero_growth_is_the_identity_at_any_horizon() {
        let base = euro(12_348);
        for steps in [1_u32, 10, 40, MAX_STEPS] {
            assert_eq!(compound_to_euro(base, Rate::ZERO, steps).unwrap(), base);
        }
    }

    /// One step of 2 % on the 2026 Grundfreibetrag: 12 348 → 12 595.
    #[test]
    fn one_step_applies_the_rate_and_rounds() {
        // 12 348 x 1.02 = 12 594.96, rounded to 12 595.
        assert_eq!(
            compound_to_euro(euro(12_348), rate(2_000), 1).unwrap(),
            euro(12_595)
        );
    }

    /// Compounding must exceed simple growth, because each year's rounded figure is the
    /// next year's base. Ten years at 2 % is about 21.9 %, not 20 %.
    #[test]
    fn growth_compounds_rather_than_accumulating_linearly() {
        let base = euro(10_000);
        let compounded = compound_to_euro(base, rate(2_000), 10).unwrap();
        assert!(compounded > euro(12_000), "got {}", compounded.cents());
        assert!(compounded < euro(12_200), "got {}", compounded.cents());
    }

    /// Monotonic in the horizon, for any positive rate.
    #[test]
    fn growth_is_monotonic_in_the_horizon() {
        let base = euro(50_000);
        let mut previous = base;
        for steps in 1_u32..=60 {
            let value = compound_to_euro(base, rate(2_800), steps).unwrap();
            assert!(
                value >= previous,
                "fell at {steps} steps: {} then {}",
                previous.cents(),
                value.cents()
            );
            previous = value;
        }
    }

    /// Deflation shrinks the amount, and cannot take it below zero.
    #[test]
    fn negative_growth_shrinks_without_going_negative() {
        let base = euro(50_000);
        let shrunk = compound_to_euro(base, rate(-2_000), 40).unwrap();
        assert!(shrunk < base);
        assert!(!shrunk.is_negative());
    }

    /// Beyond the bound the loop refuses rather than running unbounded.
    #[test]
    fn an_excessive_horizon_is_refused() {
        assert_eq!(
            compound_to_euro(euro(1_000), rate(2_000), MAX_STEPS.saturating_add(1)),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn snapping_lands_on_the_grid() {
        // 600-euro grid, as § 159 SGB VI uses for the pension ceiling.
        assert_eq!(snap_to_multiple(euro(101_401), 600).unwrap(), euro(101_400));
        // A tie on the grid rounds up, as commercial rounding requires.
        assert_eq!(snap_to_multiple(euro(101_700), 600).unwrap(), euro(102_000));
        assert_eq!(snap_to_multiple(euro(101_701), 600).unwrap(), euro(102_000));
        // 450-euro grid, as § 6 Abs. 7 SGB V uses for the health ceiling.
        assert_eq!(snap_to_multiple(euro(69_600), 450).unwrap(), euro(69_750));
    }

    #[test]
    fn snapping_refuses_a_zero_grid() {
        assert_eq!(
            snap_to_multiple(euro(1_000), 0),
            Err(MoneyError::DivisionByZero)
        );
    }

    /// Every step of a ceiling projection must stay on the statutory grid, not just the
    /// final one. A figure off the grid would be visibly invented.
    #[test]
    fn ceiling_projections_stay_on_the_grid_at_every_step() {
        for (base, grid) in [(101_400_i64, 600_i64), (69_750, 450)] {
            for steps in 0_u32..=40 {
                let value = compound_to_multiple(euro(base), rate(2_800), steps, grid).unwrap();
                let euros = value.whole_euro_floor().unwrap();
                assert_eq!(
                    euros % grid,
                    0,
                    "{euros} is not a multiple of {grid} after {steps} steps"
                );
            }
        }
    }

    /// The mechanism reproduces the enacted 2026 ceilings from the 2025 ones when given
    /// the wage growth the SVBezGrV 2026 actually cites (5.16 %).
    ///
    /// This validates the growth-plus-snap logic against real statutory output, which
    /// is a much stronger check than any internal consistency property. It does *not*
    /// claim that a single wage-growth assumption reproduces every series — the
    /// Durchschnittsentgelt uses a differently-lagged basis and grew 2.87 % over the
    /// same period, which is exactly why the crate documents one rate for all series as
    /// a simplification.
    #[test]
    fn the_mechanism_reproduces_the_enacted_2026_ceilings() {
        let observed_wage_growth = rate(5_160);

        // Pension and unemployment: 96 600 -> 101 400, on the 600-euro grid.
        assert_eq!(
            compound_to_multiple(euro(96_600), observed_wage_growth, 1, 600).unwrap(),
            euro(101_400),
            "the projected pension ceiling should reproduce the enacted figure"
        );

        // Health and care: 66 150 -> 69 750, on the 450-euro grid.
        assert_eq!(
            compound_to_multiple(euro(66_150), observed_wage_growth, 1, 450).unwrap(),
            euro(69_750),
            "the projected health ceiling should reproduce the enacted figure"
        );
    }

    /// The Rentenwert is stated to the cent, so it must compound at cent precision.
    #[test]
    fn the_pension_value_compounds_at_cent_precision() {
        let base = Money::from_euro_cents(42, 52).expect("valid");
        let grown = compound_to_cent(base, rate(2_800), 1).unwrap();
        // 42.52 x 1.028 = 43.71056, rounded to 43.71.
        assert_eq!(grown.cents(), 4_371);
    }
}
