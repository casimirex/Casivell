//! The Progressionsvorbehalt of § 32b EStG.
//!
//! # Tax-free is not free
//!
//! Elterngeld, Arbeitslosengeld, Kurzarbeitergeld and Krankengeld are not taxed. But § 32b
//! adds them to the taxable income for the sole purpose of finding a *rate*, and then applies
//! that rate to the income that is actually taxable. The benefit itself stays untaxed; it
//! raises the price of every other euro.
//!
//! The effect is routinely underestimated. A household on 40 000 € that receives 10 000 € of
//! Elterngeld pays the rate of a 50 000 € earner on its 40 000 €. Nothing on any payslip
//! warns of it, the benefit arrives untaxed all year, and the demand lands with the
//! Steuerbescheid — which is why a projection that showed parental leave without it would
//! mislead precisely the households that most need the number.
//!
//! # Two details the statute puts in easily missed places
//!
//! **The benefits are reduced by any unused Arbeitnehmer-Pauschbetrag.** § 32b Abs. 2 Satz 1
//! Nr. 1 deducts the § 9a Pauschbetrag from the benefits "soweit er nicht bei der Ermittlung
//! der Einkünfte aus nichtselbständiger Arbeit abziehbar ist" — that is, only the part the
//! employment income did not already absorb. Someone who worked all year gets nothing further;
//! someone who worked one month gets most of the 1 230 € set against their benefits. Ignoring
//! this overstates the rate for exactly the people the provision protects.
//!
//! **There is no statutory rounding rule for the rate.** § 32b Abs. 2 says what to compute and
//! not to how many places, and § 32a's rounding rules govern its own inputs and outputs rather
//! than this ratio. Administrative practice computes the rate to four decimal places;
//! [`progression_tax`] instead applies the proportion exactly in `i128` and rounds once at the
//! end, which cannot differ from the rounded-rate result by more than a euro or two. The
//! choice is stated here rather than buried, because it is a choice.
//!
//! # Not modelled
//!
//! The negative branch — § 32b Abs. 1 Nr. 2 and 3, where foreign income *reduces* the rate
//! base — and the special rates of Abs. 2 Satz 2 for Abs. 1 Nr. 2 cases. This module handles
//! the Nr. 1 wage-replacement benefits, which is what a German household simulator meets.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_lawdata::{DeductionParameters, IncomeTaxTariff};
use casivell_tax::{FilingStatus, income_tax};

/// The effect of the Progressionsvorbehalt on a year's tax.
///
/// Every stage is reported, because the whole point is to show a household *why* its tax rose
/// when its taxable income did not — and a single output figure cannot do that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progression {
    /// The zu versteuerndes Einkommen, unchanged by this provision.
    pub taxable_income: Money,
    /// The benefits received, before the § 9a reduction.
    pub benefits: Money,
    /// The Arbeitnehmer-Pauschbetrag set against the benefits, per Abs. 2 Nr. 1.
    pub lump_sum_applied: Money,
    /// The benefits as they enter the rate base, after that reduction.
    pub rate_addition: Money,
    /// The notional income the rate is taken from: taxable income plus the above.
    pub rate_base: Money,

    /// The special rate, reported to the nearest part per million.
    ///
    /// For display only. The tax below is computed from the exact proportion rather than
    /// from this rounded figure — see the module documentation.
    pub special_rate: Rate,

    /// Tax at the special rate, on the taxable income alone.
    pub income_tax: Money,
    /// What the tax would have been had the benefits not existed.
    pub tax_without_benefits: Money,
    /// The extra tax the benefits caused — the number a household actually wants.
    pub cost: Money,
}

impl Progression {
    /// Whether the Progressionsvorbehalt changed anything.
    ///
    /// False when no benefits were received, and also when the Pauschbetrag absorbed all of
    /// them — a real case for a small benefit in a year with no employment income.
    #[must_use]
    pub const fn applies(&self) -> bool {
        !self.rate_addition.is_zero()
    }
}

/// Applies § 32b to a year's taxable income.
///
/// `benefits` is the sum of the Abs. 1 Nr. 1 wage-replacement benefits received in the year.
/// `employment_income` is the Bruttoarbeitslohn, needed only to work out how much of the § 9a
/// Pauschbetrag the employment income already absorbed.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn progression_tax(
    taxable_income: Money,
    benefits: Money,
    employment_income: Money,
    filing: FilingStatus,
    tariff: &IncomeTaxTariff,
    deductions: &DeductionParameters,
) -> Result<Progression, MoneyError> {
    let taxable_income = taxable_income.floor_at_zero();
    let benefits = benefits.floor_at_zero();
    let tax_without_benefits = income_tax(taxable_income, tariff, filing)?.income_tax;

    // Abs. 2 Nr. 1: only the part of the Pauschbetrag the employment income could not use.
    // § 9a caps the deduction at the income itself, so an employee earning less than the
    // Pauschbetrag leaves the remainder available here.
    let lump_sum = deductions.employee_lump_sum;
    let absorbed = lump_sum.min(employment_income.floor_at_zero());
    let unused = lump_sum.sub(absorbed)?.floor_at_zero();
    let lump_sum_applied = unused.min(benefits);
    let rate_addition = benefits.sub(lump_sum_applied)?;

    let rate_base = taxable_income.add(rate_addition)?;
    let tax_on_base = income_tax(rate_base, tariff, filing)?.income_tax;

    // The rate applied to the real income. Expressed as a proportion rather than a rounded
    // rate: `tax_on_base × taxable_income ÷ rate_base` is what "the rate that results" means,
    // and taking it exactly avoids quantising a figure the statute gives no rule for.
    //
    // Floor, matching § 32a Abs. 1's own *abrunden* of the tax it produces.
    let income_tax = if rate_base.is_zero() {
        Money::ZERO
    } else {
        tax_on_base
            .mul_div(taxable_income, rate_base, Rounding::Floor)?
            .floor_to_euro()?
    };

    Ok(Progression {
        taxable_income,
        benefits,
        lump_sum_applied,
        rate_addition,
        rate_base,
        special_rate: average_rate(tax_on_base, rate_base)?,
        income_tax,
        tax_without_benefits,
        // Floored at zero: the provision can only raise the rate, and a negative figure
        // would be a rounding artefact rather than a refund.
        cost: income_tax.sub(tax_without_benefits)?.floor_at_zero(),
    })
}

/// The average rate `tax / income`, to the nearest part per million.
///
/// Reporting only. Zero income means zero rate rather than a division error, which is the
/// answer a report wants for a household that owed nothing.
fn average_rate(tax: Money, income: Money) -> Result<Rate, MoneyError> {
    if income.is_zero() {
        return Ok(Rate::ZERO);
    }
    let scaled = i128::from(tax.cents()).saturating_mul(i128::from(Rate::ONE.ppm()));
    let ppm = scaled
        .checked_div(i128::from(income.cents()))
        .ok_or(MoneyError::DivisionByZero)?;
    Rate::from_ppm(i64::try_from(ppm).map_err(|_| MoneyError::Overflow)?)
}

#[cfg(test)]
mod tests {
    use super::{Progression, progression_tax};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{DeductionParameters, IncomeTaxTariff};
    use casivell_tax::{FilingStatus, income_tax};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn tariff() -> IncomeTaxTariff {
        IncomeTaxTariff::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn deductions() -> DeductionParameters {
        DeductionParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn compute(zve: i64, benefits: i64, employment: i64, filing: FilingStatus) -> Progression {
        progression_tax(
            euro(zve),
            euro(benefits),
            euro(employment),
            filing,
            &tariff(),
            &deductions(),
        )
        .expect("computes")
    }

    /// A household with a full year of employment income: the Pauschbetrag is already spent,
    /// so the whole benefit enters the rate base.
    fn worked_all_year(zve: i64, benefits: i64) -> Progression {
        compute(zve, benefits, 60_000, FilingStatus::Individual)
    }

    // ---------------------------------------------------------------------
    // The mechanism
    // ---------------------------------------------------------------------

    /// No benefits must mean no change at all. The provision has to be inert when it does not
    /// apply, or every household would pay for its existence.
    #[test]
    fn without_benefits_nothing_changes() {
        for zve in [0_i64, 15_000, 40_000, 120_000] {
            let result = worked_all_year(zve, 0);
            assert!(!result.applies());
            assert_eq!(result.income_tax, result.tax_without_benefits);
            assert_eq!(result.cost, Money::ZERO);
        }
    }

    /// The benefit itself is never taxed: the tax is levied on the taxable income alone, and
    /// must stay below the tax on income plus benefit.
    #[test]
    fn the_benefit_is_not_taxed_only_the_rate_rises() {
        let result = worked_all_year(40_000, 10_000);
        assert!(result.applies());

        let tax_if_benefits_were_taxable =
            income_tax(euro(50_000), &tariff(), FilingStatus::Individual)
                .unwrap()
                .income_tax;

        assert!(
            result.income_tax > result.tax_without_benefits,
            "the rate rose"
        );
        assert!(
            result.income_tax < tax_if_benefits_were_taxable,
            "but the benefit was not itself taxed"
        );
    }

    /// The rate applied must be the rate belonging to the *combined* income, which is the
    /// whole substance of the provision. Checked against the average rate on the rate base
    /// rather than against a transcribed figure.
    #[test]
    fn the_rate_is_the_one_belonging_to_the_combined_income() {
        let result = worked_all_year(40_000, 10_000);

        let plain_rate = {
            let tax = income_tax(euro(40_000), &tariff(), FilingStatus::Individual)
                .unwrap()
                .income_tax;
            i128::from(tax.cents()) * 1_000_000 / i128::from(euro(40_000).cents())
        };
        assert!(
            i128::from(result.special_rate.ppm()) > plain_rate,
            "the special rate must exceed the household's own average rate"
        );

        // And applying it to the taxable income reproduces the tax, to within the euro the
        // final rounding permits.
        let reconstructed = euro(40_000)
            .mul_rate(result.special_rate, casivell_core::Rounding::Floor)
            .unwrap();
        assert!(
            (reconstructed.cents() - result.income_tax.cents()).abs() < 200,
            "reconstructed {reconstructed:?} against {:?}",
            result.income_tax
        );
    }

    /// The cost must grow with the benefit, and never turn into a saving.
    #[test]
    fn the_cost_grows_with_the_benefit_and_is_never_negative() {
        let mut previous = Money::ZERO;
        for benefit in [0_i64, 2_000, 5_000, 10_000, 20_000, 30_000] {
            let result = worked_all_year(40_000, benefit);
            assert!(!result.cost.is_negative());
            assert!(
                result.cost >= previous,
                "the cost fell when the benefit rose to {benefit}"
            );
            previous = result.cost;
        }
        assert!(previous > Money::ZERO);
    }

    /// § 32b applies the **average** rate, not the marginal one — and the average keeps
    /// rising even where the marginal rate has gone flat.
    ///
    /// This is worth stating because the intuitive expectation is the opposite: a top-rate
    /// earner already pays 45 % at the margin, so surely more benefit cannot cost more? It
    /// can. At 400 000 € the average rate is 40,25 %, and adding benefit pulls it toward 45 %,
    /// which costs real money on all 400 000 € — 474 € for a 10 000 € benefit, 1 770 € for
    /// 40 000 €. The provision has no income above which it stops applying.
    ///
    /// What it does have is a ceiling: the average rate can approach the top marginal rate but
    /// never pass it, so the cost is bounded by the gap between the two applied to the whole
    /// income. Both halves are asserted, because a model that let the rate exceed 45 % would
    /// be producing tax the tariff cannot levy.
    #[test]
    fn the_average_rate_keeps_rising_even_where_the_marginal_rate_is_flat() {
        let small = worked_all_year(400_000, 10_000);
        let large = worked_all_year(400_000, 40_000);

        assert!(
            large.cost > small.cost,
            "the cost must keep growing in the flat zone: {:?} then {:?}",
            small.cost,
            large.cost
        );

        // Bounded by the top marginal rate: the special rate can approach it, never exceed it.
        let top = tariff().upper_proportional.marginal_rate;
        for result in [small, large] {
            assert!(
                result.special_rate.ppm() < top.ppm(),
                "the special rate {:?} passed the Spitzensteuersatz",
                result.special_rate
            );
        }
        // And the cost stays under what closing the whole gap to the top rate would cost.
        let ceiling = euro(400_000)
            .mul_rate(top, casivell_core::Rounding::Ceiling)
            .unwrap()
            .sub(large.tax_without_benefits)
            .unwrap();
        assert!(large.cost < ceiling);
    }

    // ---------------------------------------------------------------------
    // The § 9a reduction, which is the easily missed part
    // ---------------------------------------------------------------------

    /// Someone who worked all year has already spent the Pauschbetrag, so none of it is
    /// available against the benefits.
    #[test]
    fn a_full_year_of_work_leaves_no_lump_sum_for_the_benefits() {
        let result = worked_all_year(40_000, 10_000);
        assert_eq!(result.lump_sum_applied, Money::ZERO);
        assert_eq!(result.rate_addition, euro(10_000));
    }

    /// Someone with no employment income at all keeps the whole Pauschbetrag, and it comes
    /// off the benefits. This is the case the provision exists to soften, and getting it
    /// wrong would overstate the rate for a household on benefits for a full year.
    #[test]
    fn no_employment_income_sets_the_whole_lump_sum_against_the_benefits() {
        let result = compute(0, 18_000, 0, FilingStatus::Individual);
        assert_eq!(result.lump_sum_applied, deductions().employee_lump_sum);
        assert_eq!(
            result.rate_addition,
            euro(18_000).sub(deductions().employee_lump_sum).unwrap()
        );
    }

    /// A part year splits it: employment income below the Pauschbetrag absorbs only what it
    /// can, and the remainder falls to the benefits.
    #[test]
    fn a_partial_year_splits_the_lump_sum() {
        let result = compute(0, 18_000, 500, FilingStatus::Individual);
        let expected = deductions().employee_lump_sum.sub(euro(500)).unwrap();
        assert_eq!(result.lump_sum_applied, expected);
        assert!(result.lump_sum_applied > Money::ZERO);
        assert!(result.lump_sum_applied < deductions().employee_lump_sum);
    }

    /// The reduction can never exceed the benefits themselves, or a small benefit would
    /// produce negative progression income.
    #[test]
    fn the_reduction_cannot_exceed_the_benefits() {
        let result = compute(30_000, 400, 0, FilingStatus::Individual);
        assert_eq!(result.lump_sum_applied, euro(400));
        assert_eq!(result.rate_addition, Money::ZERO);
        assert!(!result.applies());
        assert_eq!(result.income_tax, result.tax_without_benefits);
    }

    // ---------------------------------------------------------------------
    // Filing status and edges
    // ---------------------------------------------------------------------

    /// The Splittingtarif must flow through, since a couple on parental leave is the
    /// commonest case of all.
    #[test]
    fn the_splitting_tariff_is_respected() {
        let joint = compute(80_000, 20_000, 120_000, FilingStatus::JointSplitting);
        let single = compute(80_000, 20_000, 120_000, FilingStatus::Individual);
        assert!(
            joint.income_tax < single.income_tax,
            "splitting must still halve the burden under the Progressionsvorbehalt"
        );
        assert!(joint.cost > Money::ZERO);
    }

    /// A household below the Grundfreibetrag owes nothing, and the provision must not
    /// conjure tax out of a benefit — the rate applies to a taxable income of zero.
    #[test]
    fn a_household_below_the_grundfreibetrag_still_owes_nothing() {
        let result = compute(0, 15_000, 0, FilingStatus::Individual);
        assert_eq!(result.income_tax, Money::ZERO);
        assert_eq!(result.cost, Money::ZERO);
    }

    /// But a household just above it feels the provision immediately, which is where the
    /// surprise demands come from.
    #[test]
    fn a_household_just_above_it_feels_the_provision_at_once() {
        let result = compute(14_000, 15_000, 14_000, FilingStatus::Individual);
        assert!(result.cost > Money::ZERO);
        // The rate applied is far above the household's own, which is the whole complaint.
        assert!(result.special_rate.ppm() > 100_000, "over 10 %");
    }

    #[test]
    fn negative_inputs_are_floored_rather_than_propagated() {
        let result = progression_tax(
            Money::from_cents(-5_000).unwrap(),
            Money::from_cents(-5_000).unwrap(),
            Money::ZERO,
            FilingStatus::Individual,
            &tariff(),
            &deductions(),
        )
        .expect("computes");
        assert_eq!(result.taxable_income, Money::ZERO);
        assert_eq!(result.benefits, Money::ZERO);
        assert_eq!(result.income_tax, Money::ZERO);
    }

    /// Every figure must reconcile: the rate base is the sum of its parts, and the cost is
    /// the difference it claims to be.
    #[test]
    fn the_parts_reconcile() {
        for (zve, benefit, employment) in [
            (40_000_i64, 10_000_i64, 60_000_i64),
            (0, 18_000, 0),
            (90_000, 3_000, 90_000),
        ] {
            let r = compute(zve, benefit, employment, FilingStatus::Individual);
            assert_eq!(r.rate_base, r.taxable_income.add(r.rate_addition).unwrap());
            assert_eq!(r.rate_addition, r.benefits.sub(r.lump_sum_applied).unwrap());
            assert_eq!(
                r.cost,
                r.income_tax
                    .sub(r.tax_without_benefits)
                    .unwrap()
                    .floor_at_zero()
            );
        }
    }

    #[test]
    fn a_zero_rate_base_reports_a_zero_rate_rather_than_failing() {
        let result = compute(0, 0, 0, FilingStatus::Individual);
        assert_eq!(result.special_rate, Rate::ZERO);
        assert_eq!(result.income_tax, Money::ZERO);
    }
}
