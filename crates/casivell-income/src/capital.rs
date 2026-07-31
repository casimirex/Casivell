//! Capital income: the Abgeltungsteuer of § 32d EStG and the § 32d Abs. 6 election.
//!
//! # Why capital income is not just more income
//!
//! Every other euro a household earns runs through the progressive tariff. Capital income does
//! not: § 32d Abs. 1 taxes it at a **flat 25 %**, outside the tariff entirely, plus
//! Solidaritätszuschlag and church tax on that. Adding it to the taxable income would be
//! wrong in both directions — too much tax for a high earner, too little for a low one.
//!
//! # The election, which is the interesting part
//!
//! A flat rate is a bad deal for anyone whose marginal rate is below it. § 32d Abs. 6 therefore
//! lets a taxpayer elect the *ordinary* tariff instead — the **Günstigerprüfung** — and the
//! tax office applies whichever is cheaper.
//!
//! The crossover is not at 25 % of income, which is the intuitive but wrong answer. It is
//! where the *marginal* rate on the additional capital income reaches 25 %, and because the
//! Sparer-Pauschbetrag exempts the first 1 000 € under either route, the comparison is between
//! two whole computations rather than two rates. [`capital_income_tax`] does both and keeps the
//! better one, as the tax office would.
//!
//! # The Sparer-Pauschbetrag is a flat exemption, not a floor
//!
//! Unlike the Werbungskosten- and Sonderausgaben-Pauschbeträge, which are floors that actual
//! expenses may exceed, § 20 Abs. 9 is a *cap on exemption*: the first 1 000 € of capital income
//! is untaxed and only the excess is taxed. Actual expenses of earning capital income are
//! **not** deductible at all beyond it (§ 20 Abs. 9 Satz 1), which is a real and often
//! surprising restriction — custody fees reduce nothing.

use crate::assessment::AssessmentLaw;
use casivell_core::{Money, MoneyError, Rounding};
use casivell_lawdata::Bundesland;
use casivell_tax::{FilingStatus, income_tax, solidarity_surcharge};

/// Which route § 32d took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapitalRoute {
    /// § 32d Abs. 1: the flat 25 % rate.
    FlatRate,
    /// § 32d Abs. 6: the ordinary tariff, elected because it is cheaper.
    ///
    /// Carries how much the election saved, which is the figure a taxpayer wants to see —
    /// electing is something they must actually do on the return, and knowing it is worth
    /// nothing is as useful as knowing it is worth something.
    OrdinaryTariff {
        /// What the election saved against the flat rate.
        saving: Money,
    },
}

/// The tax on a household's capital income.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapitalIncomeTax {
    /// Gross capital income for the year.
    pub gross: Money,
    /// The Sparer-Pauschbetrag applied.
    pub allowance_applied: Money,
    /// The taxable remainder after the allowance.
    pub taxable: Money,

    /// Income tax on the capital income, by whichever route was cheaper.
    pub income_tax: Money,
    /// Solidaritätszuschlag attributable to it.
    pub solidarity_surcharge: Money,
    /// Church tax attributable to it.
    pub church_tax: Money,
    /// The total attributable to the capital income.
    pub total: Money,

    /// Which route § 32d took, and what the election was worth.
    pub route: CapitalRoute,
}

/// Computes the tax on capital income, taking the cheaper of the two § 32d routes.
///
/// `other_taxable_income` is the household's taxable income from everything else — needed
/// because the ordinary-tariff route stacks the capital income on top of it, so the marginal
/// rate that decides the election depends on it.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn capital_income_tax(
    gross_capital_income: Money,
    other_taxable_income: Money,
    filing: FilingStatus,
    church: Option<Bundesland>,
    law: &AssessmentLaw,
) -> Result<CapitalIncomeTax, MoneyError> {
    let gross = gross_capital_income.floor_at_zero();

    // § 20 Abs. 9: the allowance doubles for a joint assessment.
    let allowance = match filing {
        FilingStatus::Individual => law.deductions.saver_allowance,
        FilingStatus::JointSplitting => law.deductions.saver_allowance.mul_int(2)?,
    };
    let allowance_applied = allowance.min(gross);
    let taxable = gross.sub(allowance_applied)?;

    // § 32d Abs. 1: the flat rate, outside the tariff.
    let flat = taxable.mul_rate(law.deductions.capital_income_rate, Rounding::Floor)?;

    // § 32d Abs. 6: the ordinary tariff, applied to the capital income stacked on top of
    // everything else. The attributable tax is the *difference* the capital income makes, not
    // the tax on it in isolation — stacking is what makes the marginal rate the right one.
    let without = income_tax(other_taxable_income, &law.tariff, filing)?.income_tax;
    let with = income_tax(other_taxable_income.add(taxable)?, &law.tariff, filing)?.income_tax;
    let tariff_route = with.sub(without)?;

    let (assessed, route) = if tariff_route < flat {
        (
            tariff_route,
            CapitalRoute::OrdinaryTariff {
                saving: flat.sub(tariff_route)?,
            },
        )
    } else {
        (flat, CapitalRoute::FlatRate)
    };

    // The surcharges follow the income tax attributable to the capital income. Computing the
    // Soli on this slice alone rather than on the household's whole liability is an
    // approximation: the Freigrenze applies to the total, so a household near it will see a
    // slightly different figure on its Steuerbescheid. Documented rather than hidden.
    let solidarity_amount = solidarity_surcharge(assessed, &law.solidarity, filing)?.amount;
    let church_amount = match church {
        Some(land) => assessed.mul_rate(law.church.rate_in(land), Rounding::Floor)?,
        None => Money::ZERO,
    };

    Ok(CapitalIncomeTax {
        gross,
        allowance_applied,
        taxable,
        income_tax: assessed,
        solidarity_surcharge: solidarity_amount,
        church_tax: church_amount,
        total: assessed.add(solidarity_amount)?.add(church_amount)?,
        route,
    })
}

#[cfg(test)]
mod tests {
    use super::{AssessmentLaw, CapitalRoute, capital_income_tax};
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::{
        Bundesland, ChurchTaxParameters, DeductionParameters, ExtraordinaryBurdenParameters,
        IncomeTaxTariff, SolidarityParameters,
    };
    use casivell_tax::FilingStatus;

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn compute(
        capital: i64,
        other: i64,
        filing: FilingStatus,
        church: Option<Bundesland>,
    ) -> super::CapitalIncomeTax {
        let year = TaxYear::new(2026).unwrap();
        let law = AssessmentLaw {
            tariff: IncomeTaxTariff::for_year(year).unwrap(),
            solidarity: SolidarityParameters::for_year(year).unwrap(),
            church: ChurchTaxParameters::for_year(year).unwrap(),
            deductions: DeductionParameters::for_year(year).unwrap(),
            burden: ExtraordinaryBurdenParameters::for_year(year).unwrap(),
        };
        capital_income_tax(euro(capital), euro(other), filing, church, &law).expect("computes")
    }

    // ---------------------------------------------------------------------
    // The Sparer-Pauschbetrag
    // ---------------------------------------------------------------------

    /// Capital income below the allowance is untaxed entirely.
    #[test]
    fn income_below_the_allowance_is_untaxed() {
        let result = compute(800, 60_000, FilingStatus::Individual, None);
        assert_eq!(result.allowance_applied, euro(800));
        assert_eq!(result.taxable, Money::ZERO);
        assert_eq!(result.total, Money::ZERO);
    }

    /// Only the excess over the allowance is taxed — it is a flat exemption, not a floor that
    /// actual expenses could exceed.
    #[test]
    fn only_the_excess_over_the_allowance_is_taxed() {
        let result = compute(3_000, 60_000, FilingStatus::Individual, None);
        assert_eq!(result.allowance_applied, euro(1_000));
        assert_eq!(result.taxable, euro(2_000));
        // 25 % of 2 000 EUR.
        assert_eq!(result.income_tax, euro(500));
        assert_eq!(result.route, CapitalRoute::FlatRate);
    }

    /// § 20 Abs. 9: the allowance doubles for a joint assessment.
    #[test]
    fn the_allowance_doubles_for_a_joint_assessment() {
        let joint = compute(3_000, 120_000, FilingStatus::JointSplitting, None);
        assert_eq!(joint.allowance_applied, euro(2_000));
        assert_eq!(joint.taxable, euro(1_000));
    }

    // ---------------------------------------------------------------------
    // The § 32d Abs. 6 election
    // ---------------------------------------------------------------------

    /// A high earner pays the flat 25 %, because their marginal rate exceeds it. This is what
    /// the Abgeltungsteuer exists for.
    #[test]
    fn a_high_earner_takes_the_flat_rate() {
        let result = compute(10_000, 150_000, FilingStatus::Individual, None);
        assert_eq!(result.route, CapitalRoute::FlatRate);
        // 25 % of 9 000 EUR.
        assert_eq!(result.income_tax, euro(2_250));
    }

    /// A low earner elects the ordinary tariff, because their marginal rate is below 25 %.
    /// Failing to offer the election would overtax them, which is exactly what § 32d Abs. 6
    /// exists to prevent.
    ///
    /// 15 000 EUR sits in zone 2, where the marginal rate is about 19 %. Note that 20 000 EUR is
    /// already *too high* for the election to win: the marginal rate there is 24.7 %, and
    /// stacking the capital income on top carries the average over 25 %. The election's value
    /// runs out earlier than the intuitive "25 % of income".
    #[test]
    fn a_low_earner_elects_the_ordinary_tariff() {
        let result = compute(5_000, 15_000, FilingStatus::Individual, None);
        let CapitalRoute::OrdinaryTariff { saving } = result.route else {
            panic!("a 15 000 EUR earner should elect the tariff");
        };
        assert!(saving > Money::ZERO);
        // And the tax is genuinely below the flat 25 % of 4 000 EUR.
        assert!(result.income_tax < euro(1_000));
    }

    /// Someone with no other income at all pays almost nothing: the capital income falls
    /// largely inside the Grundfreibetrag under the elected tariff.
    #[test]
    fn with_no_other_income_the_grundfreibetrag_absorbs_it() {
        let result = compute(8_000, 0, FilingStatus::Individual, None);
        assert!(matches!(result.route, CapitalRoute::OrdinaryTariff { .. }));
        assert_eq!(
            result.income_tax,
            Money::ZERO,
            "7 000 EUR is below the Grundfreibetrag"
        );
    }

    /// The office always applies the cheaper route, at every income. This is the property that
    /// matters, and it holds without reference to where the crossover sits.
    #[test]
    fn the_cheaper_route_always_wins() {
        for filing in [FilingStatus::Individual, FilingStatus::JointSplitting] {
            let mut other = 0_i64;
            while other <= 250_000 {
                let result = compute(10_000, other, filing, None);
                let flat_would_be = euro(match filing {
                    FilingStatus::Individual => 9_000,
                    FilingStatus::JointSplitting => 8_000,
                })
                .mul_rate(
                    casivell_core::Rate::from_percent_millis(25_000).unwrap(),
                    casivell_core::Rounding::Floor,
                )
                .unwrap();
                assert!(
                    result.income_tax <= flat_would_be,
                    "{filing:?} at {other}: paid more than the flat rate"
                );
                other = other.saturating_add(6_100);
            }
        }
    }

    /// The crossover moves with the other income, because the election depends on the *marginal*
    /// rate and not on the capital income in isolation. A model comparing 25 % against an
    /// average rate would put it in the wrong place.
    #[test]
    fn the_crossover_depends_on_the_other_income() {
        let mut crossover = 0_i64;
        let mut other = 0_i64;
        while other <= 120_000 {
            if compute(10_000, other, FilingStatus::Individual, None).route
                == CapitalRoute::FlatRate
            {
                crossover = other;
                break;
            }
            other = other.saturating_add(500);
        }
        // Well inside zone 3, and well below the 42 % threshold — the flat rate starts winning
        // long before a household would call itself a high earner.
        assert!(
            (10_000..=30_000).contains(&crossover),
            "the crossover came out at {crossover}"
        );
    }

    // ---------------------------------------------------------------------
    // Surcharges and properties
    // ---------------------------------------------------------------------

    #[test]
    fn church_tax_follows_the_attributable_income_tax() {
        let secular = compute(10_000, 150_000, FilingStatus::Individual, None);
        assert_eq!(secular.church_tax, Money::ZERO);

        let bavarian = compute(
            10_000,
            150_000,
            FilingStatus::Individual,
            Some(Bundesland::Bayern),
        );
        let rhenish = compute(
            10_000,
            150_000,
            FilingStatus::Individual,
            Some(Bundesland::NordrheinWestfalen),
        );
        // 8 % of the flat tax in Bavaria against 9 % elsewhere.
        assert_eq!(bavarian.church_tax, euro(180));
        assert_eq!(
            rhenish.church_tax,
            euro(202).add(Money::from_cents(50).unwrap()).unwrap()
        );
        assert!(bavarian.church_tax < rhenish.church_tax);
    }

    #[test]
    fn the_total_is_the_sum_of_its_parts() {
        let result = compute(
            10_000,
            150_000,
            FilingStatus::Individual,
            Some(Bundesland::Berlin),
        );
        assert_eq!(
            result.total,
            result
                .income_tax
                .add(result.solidarity_surcharge)
                .unwrap()
                .add(result.church_tax)
                .unwrap()
        );
    }

    /// Monotonic in the capital income, and never exceeding it.
    #[test]
    fn the_tax_is_monotonic_and_bounded_by_the_income() {
        let mut previous = Money::ZERO;
        let mut capital = 0_i64;
        while capital <= 200_000 {
            let result = compute(capital, 60_000, FilingStatus::Individual, None);
            assert!(result.total >= previous, "the tax fell at {capital}");
            assert!(
                result.total <= euro(capital),
                "the tax exceeded the income at {capital}"
            );
            previous = result.total;
            capital = capital.saturating_add(4_700);
        }
    }

    #[test]
    fn no_capital_income_means_no_tax() {
        let result = compute(
            0,
            60_000,
            FilingStatus::Individual,
            Some(Bundesland::Berlin),
        );
        assert_eq!(result.total, Money::ZERO);
        assert_eq!(result.allowance_applied, Money::ZERO);
    }
}
