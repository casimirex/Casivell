//! Church tax (Kirchensteuer).
//!
//! 8 % of assessed income tax in Baden-Württemberg and Bayern, 9 % in the other
//! fourteen states. The rate is set by each state's Kirchensteuergesetz rather
//! than federally, which is why the state — not the taxpayer's residence in some
//! broader sense — determines it.
//!
//! # Two known departures from the statute
//!
//! Both are documented on [`casivell_lawdata::ChurchTaxParameters`] and repeated
//! here because they change the number a user sees:
//!
//! - **Families are overcharged.** § 51a Abs. 2 EStG recomputes the assessment
//!   base as though the full Kinderfreibetrag had been claimed, which lowers it.
//!   Casivell uses the assessed income tax directly, so a household with children
//!   sees a church tax figure that is too high. [`ChurchTaxResult::base_is_exact`]
//!   reports this, and the UI must not present an inexact figure as final.
//! - **Kappung is not applied.** Most Landeskirchen cap church tax at 2.75 %–4 %
//!   of taxable income. The cap binds only at high incomes, is set per
//!   Landeskirche rather than per state, and in several regions must be applied
//!   for rather than being granted automatically.
//!
//! Church tax is itself deductible as a Sonderausgabe under § 10 Abs. 1 Nr. 4
//! EStG, which makes the true joint determination of income tax and church tax
//! mildly circular. That interaction belongs to the zvE-determination crate, not
//! here.

use casivell_core::{Money, MoneyError, Rounding};
use casivell_lawdata::{Bundesland, ChurchTaxParameters};

/// The outcome of a church tax calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChurchTaxResult {
    /// The church tax payable, on the simplified base described above.
    pub amount: Money,
    /// The state whose rate was applied.
    pub land: Bundesland,
    /// Whether the assessment base used is the statutorily correct one.
    ///
    /// `false` whenever the household has children, because the § 51a Abs. 2
    /// recomputation is not yet implemented. A caller must propagate this to the
    /// user rather than discarding it.
    pub base_is_exact: bool,
}

/// Computes church tax on an assessed income tax amount.
///
/// `children` is the number of children who would attract a Kinderfreibetrag. It
/// is taken only to determine [`ChurchTaxResult::base_is_exact`]; it does not yet
/// affect the amount. Passing it now means the signature will not change when
/// § 51a is implemented, so callers written today will not silently keep the wrong
/// answer.
///
/// # Errors
///
/// [`MoneyError::Overflow`] if an intermediate leaves the representable domain.
pub fn church_tax(
    income_tax: Money,
    params: &ChurchTaxParameters,
    land: Bundesland,
    children: u8,
) -> Result<ChurchTaxResult, MoneyError> {
    // A loss or a nil assessment attracts no church tax; the rate has nothing to
    // apply to.
    let base = income_tax.floor_at_zero();
    let rate = params.rate_in(land);
    let amount = base.mul_rate(rate, Rounding::Floor)?;
    Ok(ChurchTaxResult {
        amount,
        land,
        base_is_exact: children == 0,
    })
}

#[cfg(test)]
mod tests {
    use super::church_tax;
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::{Bundesland, ChurchTaxParameters};

    fn params() -> ChurchTaxParameters {
        ChurchTaxParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn tax_cents(income_tax_euro: i64, land: Bundesland) -> i64 {
        let base = Money::from_euro(income_tax_euro).unwrap();
        church_tax(base, &params(), land, 0).unwrap().amount.cents()
    }

    #[test]
    fn the_reduced_rate_applies_in_bavaria_and_baden_wuerttemberg() {
        // 8 % of 10 000 € is 800,00 €.
        assert_eq!(tax_cents(10_000, Bundesland::Bayern), 80_000);
        assert_eq!(tax_cents(10_000, Bundesland::BadenWuerttemberg), 80_000);
    }

    #[test]
    fn the_standard_rate_applies_everywhere_else() {
        // 9 % of 10 000 € is 900,00 €.
        for land in [
            Bundesland::Berlin,
            Bundesland::NordrheinWestfalen,
            Bundesland::Sachsen,
            Bundesland::Hamburg,
        ] {
            assert_eq!(tax_cents(10_000, land), 90_000, "wrong rate in {land:?}");
        }
    }

    /// Every state must yield one of exactly two figures. A state falling through
    /// to some third value would mean the rate lookup has a gap.
    #[test]
    fn every_state_yields_one_of_the_two_statutory_rates() {
        for land in Bundesland::ALL {
            let cents = tax_cents(10_000, land);
            assert!(
                cents == 80_000 || cents == 90_000,
                "{land:?} produced {cents} cents, which is neither 8 % nor 9 %"
            );
        }
    }

    #[test]
    fn a_nil_or_negative_assessment_attracts_no_church_tax() {
        assert_eq!(tax_cents(0, Bundesland::Bayern), 0);
        let loss = Money::from_euro(-5_000).unwrap();
        let result = church_tax(loss, &params(), Bundesland::Bayern, 0).unwrap();
        assert_eq!(result.amount, Money::ZERO);
    }

    /// The known-inexact flag must be set whenever children are present, and only
    /// then. Silently returning an overstated figure without the flag is the
    /// failure mode this guards against.
    #[test]
    fn the_base_is_flagged_inexact_exactly_when_children_are_present() {
        let base = Money::from_euro(10_000).unwrap();
        let childless = church_tax(base, &params(), Bundesland::Bayern, 0).unwrap();
        assert!(childless.base_is_exact);
        for children in 1_u8..=5 {
            let with_children = church_tax(base, &params(), Bundesland::Bayern, children).unwrap();
            assert!(
                !with_children.base_is_exact,
                "{children} children should mark the base inexact"
            );
            // Until § 51a is implemented the amount is unchanged, which is exactly
            // why the flag has to exist.
            assert_eq!(with_children.amount, childless.amount);
        }
    }

    #[test]
    fn the_result_reports_the_state_it_used() {
        let base = Money::from_euro(1_000).unwrap();
        let result = church_tax(base, &params(), Bundesland::Thueringen, 0).unwrap();
        assert_eq!(result.land, Bundesland::Thueringen);
    }

    /// Church tax is monotonic in the income tax it is levied on, and is always a
    /// small fraction of it.
    #[test]
    fn church_tax_is_monotonic_and_bounded_by_the_standard_rate() {
        let mut previous = 0_i64;
        let mut tax = 0_i64;
        while tax <= 200_000 {
            let cents = tax_cents(tax, Bundesland::NordrheinWestfalen);
            assert!(cents >= previous, "church tax fell at {tax} €");
            // Never more than 9 % of the base.
            assert!(cents.saturating_mul(100) <= tax.saturating_mul(900));
            previous = cents;
            tax = tax.saturating_add(179);
        }
    }
}
