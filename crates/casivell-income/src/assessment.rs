//! The annual assessment: the Günstigerprüfung, the tax, and the refund.
//!
//! # The Günstigerprüfung of § 31 EStG
//!
//! A family is supported either by Kindergeld or by the Kinderfreibetrag, never both. § 31
//! resolves it by computing both and keeping whichever leaves the household better off:
//!
//! ```text
//!   with the allowance:  tax(income − Kinderfreibetrag) + Kindergeld clawed back
//!   without it:          tax(income), Kindergeld kept
//! ```
//!
//! Because the allowance's value rises with the marginal rate while Kindergeld is a flat
//! amount, the allowance wins only above a threshold. Published commentary puts that
//! crossover at roughly **86 000 €** of income for a jointly assessed couple with one child
//! under the 2026 tariff, which
//! `the_guenstigerpruefung_crossover_matches_published_commentary` checks — an external
//! validation point for logic that has no official reference table.
//!
//! Two details worth stating because they are easy to get wrong:
//!
//! - The comparison is on the **tax saving**, not on the allowance itself. Comparing
//!   9 756 € of allowance against 3 108 € of Kindergeld would favour the allowance for
//!   everybody.
//! - When the allowance wins, the Kindergeld already received is **added back** to the tax
//!   (§ 31 Satz 5). Omitting that would double-count the support and understate the
//!   liability by the full Kindergeld.
//!
//! # The refund
//!
//! Withholding is the statute's own approximation of the annual liability, so the two rarely
//! agree exactly. The difference is what a taxpayer gets back or owes, and it is the figure
//! most people actually want from a tax tool. [`Assessment::refund`] reports it.

use casivell_core::{Money, MoneyError, Rounding};
use casivell_lawdata::{
    Bundesland, ChurchTaxParameters, DeductionParameters, IncomeTaxTariff, SolidarityParameters,
};
use casivell_tax::{FilingStatus, income_tax, solidarity_surcharge};

use crate::taxable_income::TaxableIncome;

/// Which support a family received, after the § 31 comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildRelief {
    /// No children, so the question does not arise.
    NotApplicable,
    /// Kindergeld was more favourable and is kept; no allowance is deducted.
    ChildBenefit {
        /// The Kindergeld received over the year.
        received: Money,
    },
    /// The Kinderfreibetrag was more favourable; the Kindergeld is added back.
    Allowance {
        /// The allowance deducted from the taxable income.
        deducted: Money,
        /// The Kindergeld added back to the tax under § 31 Satz 5.
        clawed_back: Money,
        /// How much better off the household is than under Kindergeld alone.
        advantage: Money,
    },
}

/// A completed annual assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// The zu versteuerndes Einkommen the tariff was applied to.
    pub taxable_income: Money,
    /// Assessed income tax.
    pub income_tax: Money,
    /// Solidaritätszuschlag.
    pub solidarity_surcharge: Money,
    /// Church tax, or zero with no affiliation.
    pub church_tax: Money,
    /// The total owed for the year.
    pub total_liability: Money,

    /// Which child support applied, and what it was worth.
    pub child_relief: ChildRelief,

    /// What was withheld during the year, as supplied by the caller.
    pub withheld: Money,
    /// Withheld less owed. Positive is a refund; negative is a further demand.
    pub refund: Money,

    /// Whether this assessment is exact or a good estimate.
    ///
    /// **Always `false` at present.** § 10's interaction has not been reconciled against a
    /// real Steuerbescheid, and außergewöhnliche Belastungen, the other six income
    /// categories and loss carry-forward are not modelled. A caller must propagate this
    /// rather than presenting the figure as a liability.
    ///
    /// Carried in the type rather than the documentation for the same reason
    /// `ChurchTaxResult::base_is_exact` is: a caveat a caller can ignore by accident is a
    /// caveat that will be ignored.
    pub is_exact: bool,
}

/// The statutory inputs an assessment needs.
#[derive(Debug, Clone, Copy)]
pub struct AssessmentLaw {
    /// The § 32a tariff.
    pub tariff: IncomeTaxTariff,
    /// Solidaritätszuschlag parameters.
    pub solidarity: SolidarityParameters,
    /// Church tax rates.
    pub church: ChurchTaxParameters,
    /// Deduction parameters, for the Kinderfreibetrag and Kindergeld.
    pub deductions: DeductionParameters,
}

/// Runs the annual assessment.
///
/// `children_tenths` counts Kinderfreibeträge in tenths, as § 32 Abs. 6 permits: `10` is one
/// full allowance and `5` the half a parent assessed individually normally holds.
///
/// `withheld` is what was actually withheld over the year — from
/// `casivell_payroll::withhold`, summed. It affects only [`Assessment::refund`].
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn assess(
    income: &TaxableIncome,
    filing: FilingStatus,
    church: Option<Bundesland>,
    children_tenths: u16,
    withheld: Money,
    law: &AssessmentLaw,
) -> Result<Assessment, MoneyError> {
    let allowance = child_allowance(children_tenths, &law.deductions)?;
    let benefit = child_benefit(children_tenths, &law.deductions)?;

    // Without the allowance: tax on the full income, Kindergeld kept.
    let plain_tax = income_tax(income.income, &law.tariff, filing)?.income_tax;

    // With it: tax on the reduced income, plus the Kindergeld added back (§ 31 Satz 5).
    let reduced_income = income.income.sub(allowance)?.floor_at_zero();
    let reduced_tax = income_tax(reduced_income, &law.tariff, filing)?.income_tax;
    let with_allowance = reduced_tax.add(benefit)?;

    // § 31: whichever leaves the household better off. The comparison is between total
    // burdens, so the Kindergeld claw-back is already inside `with_allowance`.
    let allowance_wins = !allowance.is_zero() && with_allowance < plain_tax;

    let (taxable, assessed_tax, child_relief) = if allowance.is_zero() {
        (income.income, plain_tax, ChildRelief::NotApplicable)
    } else if allowance_wins {
        (
            reduced_income,
            with_allowance,
            ChildRelief::Allowance {
                deducted: allowance,
                clawed_back: benefit,
                advantage: plain_tax.sub(with_allowance)?,
            },
        )
    } else {
        (
            income.income,
            plain_tax,
            ChildRelief::ChildBenefit { received: benefit },
        )
    };

    // The surcharges are levied on the tax *before* the Kindergeld claw-back, and on the base
    // reduced by the Kinderfreibetrag whatever the Günstigerprüfung decided — § 51a Abs. 2
    // requires that base for both, which is the same rule the Programmablaufplan applies.
    let surcharge_base = reduced_tax;
    let solidarity = solidarity_surcharge(surcharge_base, &law.solidarity, filing)?.amount;
    let church_tax = match church {
        Some(land) => surcharge_base.mul_rate(law.church.rate_in(land), Rounding::Floor)?,
        None => Money::ZERO,
    };

    let total_liability = assessed_tax.add(solidarity)?.add(church_tax)?;

    Ok(Assessment {
        taxable_income: taxable,
        income_tax: assessed_tax,
        solidarity_surcharge: solidarity,
        church_tax,
        total_liability,
        child_relief,
        withheld,
        refund: withheld.sub(total_liability)?,
        // See the field documentation: § 10's interaction is not yet reconciled against a
        // real Steuerbescheid, so no assessment from this crate claims to be exact.
        is_exact: false,
    })
}

/// The Kinderfreibetrag for `children_tenths` tenths of a child.
fn child_allowance(
    children_tenths: u16,
    deductions: &DeductionParameters,
) -> Result<Money, MoneyError> {
    if children_tenths == 0 {
        return Ok(Money::ZERO);
    }
    deductions
        .child_allowance_total()?
        .mul_int(i64::from(children_tenths))?
        .div_int(10, Rounding::Floor)
}

/// The Kindergeld received for `children_tenths` tenths of a child.
///
/// Scaled the same way as the allowance, so a parent holding half an allowance is compared
/// against half the Kindergeld. Comparing a half allowance against the full Kindergeld would
/// wrongly favour Kindergeld for every individually assessed parent.
fn child_benefit(
    children_tenths: u16,
    deductions: &DeductionParameters,
) -> Result<Money, MoneyError> {
    if children_tenths == 0 {
        return Ok(Money::ZERO);
    }
    deductions
        .child_benefit_annual()?
        .mul_int(i64::from(children_tenths))?
        .div_int(10, Rounding::Floor)
}

#[cfg(test)]
mod tests {
    use super::{AssessmentLaw, ChildRelief, assess};
    use crate::taxable_income::{Employee, taxable_income};
    use crate::vorsorge::Contributions;
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::{
        Bundesland, ChurchTaxParameters, DeductionParameters, IncomeTaxTariff, SolidarityParameters,
    };
    use casivell_tax::FilingStatus;

    fn law() -> AssessmentLaw {
        let year = TaxYear::new(2026).unwrap();
        AssessmentLaw {
            tariff: IncomeTaxTariff::for_year(year).unwrap(),
            solidarity: SolidarityParameters::for_year(year).unwrap(),
            church: ChurchTaxParameters::for_year(year).unwrap(),
            deductions: DeductionParameters::for_year(year).unwrap(),
        }
    }

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    /// Builds a taxable income with a given Einkommen, by working backwards from a salary
    /// with no deductions beyond the mandatory lump sums.
    fn income_of(einkommen: i64) -> crate::taxable_income::TaxableIncome {
        let employee = Employee {
            // Add back the two lump sums so the resulting Einkommen is the figure asked for.
            gross_annual: euro(einkommen + 1_230 + 36),
            work_expenses: Money::ZERO,
            contributions: Contributions {
                pension_employee: Money::ZERO,
                pension_employer: Money::ZERO,
                retirement_voluntary: Money::ZERO,
                health_general: Money::ZERO,
                health_supplementary: Money::ZERO,
                care: Money::ZERO,
                other_provision: Money::ZERO,
            },
            church_tax_paid: Money::ZERO,
            other_special_expenses: Money::ZERO,
            children: 0,
        };
        taxable_income(&employee, &law().deductions).expect("computes")
    }

    // ---------------------------------------------------------------------
    // The Günstigerprüfung
    // ---------------------------------------------------------------------

    /// The external validation point. Published commentary puts the crossover for a jointly
    /// assessed couple with one child at roughly 86 000 EUR of income under the 2026 tariff.
    ///
    /// This is the strongest check available for logic with no official reference table: the
    /// figure was derived elsewhere, by someone else, from the same statute.
    #[test]
    fn the_guenstigerpruefung_crossover_matches_published_commentary() {
        let law = law();
        let relief_at = |einkommen: i64| {
            assess(
                &income_of(einkommen),
                FilingStatus::JointSplitting,
                None,
                10,
                Money::ZERO,
                &law,
            )
            .expect("assesses")
            .child_relief
        };

        // Well below: Kindergeld wins.
        assert!(matches!(
            relief_at(70_000),
            ChildRelief::ChildBenefit { .. }
        ));
        // Well above: the allowance wins.
        assert!(matches!(relief_at(100_000), ChildRelief::Allowance { .. }));

        // And the crossover sits near 86 000. Bracketed loosely, because the published figure
        // is itself given as "rund 86 000" and depends on the deductions assumed.
        let mut crossover = 0_i64;
        let mut einkommen = 70_000_i64;
        while einkommen <= 100_000 {
            if matches!(relief_at(einkommen), ChildRelief::Allowance { .. }) {
                crossover = einkommen;
                break;
            }
            einkommen = einkommen.saturating_add(250);
        }
        assert!(
            (82_000..=90_000).contains(&crossover),
            "the crossover came out at {crossover}, not near the published 86 000"
        );
    }

    /// The comparison must be on the tax *saving*, not on the allowance itself. A middle
    /// earner's allowance is worth less than the Kindergeld even though 9 756 EUR of allowance
    /// exceeds 3 108 EUR of Kindergeld — which is why the naive comparison is wrong.
    #[test]
    fn the_comparison_is_on_the_tax_saving_not_the_allowance() {
        let law = law();
        let assessment = assess(
            &income_of(40_000),
            FilingStatus::Individual,
            None,
            10,
            Money::ZERO,
            &law,
        )
        .expect("assesses");

        assert!(
            matches!(assessment.child_relief, ChildRelief::ChildBenefit { .. }),
            "Kindergeld should win at 40 000 EUR despite the larger allowance"
        );
        // The allowance is indeed larger than the Kindergeld, so a naive comparison would
        // have chosen it.
        let allowance = law.deductions.child_allowance_total().unwrap();
        let benefit = law.deductions.child_benefit_annual().unwrap();
        assert!(allowance > benefit);
    }

    /// When the allowance wins, the Kindergeld must be added back. Omitting the claw-back
    /// would understate the liability by the full Kindergeld.
    #[test]
    fn the_child_benefit_is_clawed_back_when_the_allowance_wins() {
        let law = law();
        let assessment = assess(
            &income_of(150_000),
            FilingStatus::Individual,
            None,
            10,
            Money::ZERO,
            &law,
        )
        .expect("assesses");

        let ChildRelief::Allowance {
            deducted,
            clawed_back,
            advantage,
        } = assessment.child_relief
        else {
            panic!("the allowance should win at 150 000 EUR");
        };
        assert_eq!(deducted, law.deductions.child_allowance_total().unwrap());
        assert_eq!(clawed_back, law.deductions.child_benefit_annual().unwrap());
        assert!(!advantage.is_negative());

        // And the tax must exceed the tax on the reduced income by exactly the claw-back.
        let reduced = income_of(150_000).income.sub(deducted).unwrap();
        let reduced_tax = casivell_tax::income_tax(reduced, &law.tariff, FilingStatus::Individual)
            .unwrap()
            .income_tax;
        assert_eq!(assessment.income_tax, reduced_tax.add(clawed_back).unwrap());
    }

    /// The Günstigerprüfung must never choose the worse option, at any income. This is the
    /// property that matters most, and it is checkable without any external figure.
    #[test]
    fn the_guenstigerpruefung_never_chooses_the_worse_option() {
        let law = law();
        for filing in [FilingStatus::Individual, FilingStatus::JointSplitting] {
            let mut einkommen = 0_i64;
            while einkommen <= 300_000 {
                let income = income_of(einkommen);
                let with_children =
                    assess(&income, filing, None, 10, Money::ZERO, &law).expect("assesses");
                let without =
                    assess(&income, filing, None, 0, Money::ZERO, &law).expect("assesses");

                // A household with a child must never be worse off than the same household
                // without one, counting the Kindergeld.
                let burden_with = with_children.total_liability;
                let benefit = law.deductions.child_benefit_annual().unwrap();
                assert!(
                    burden_with <= without.total_liability.add(benefit).unwrap(),
                    "{filing:?} at {einkommen}: a child made the household worse off"
                );
                einkommen = einkommen.saturating_add(4_300);
            }
        }
    }

    /// A childless household must report that the question does not arise, rather than
    /// silently choosing Kindergeld of zero.
    #[test]
    fn a_childless_household_has_no_child_relief() {
        let assessment = assess(
            &income_of(50_000),
            FilingStatus::Individual,
            None,
            0,
            Money::ZERO,
            &law(),
        )
        .expect("assesses");
        assert_eq!(assessment.child_relief, ChildRelief::NotApplicable);
        assert_eq!(assessment.taxable_income, income_of(50_000).income);
    }

    /// A parent assessed individually holds half an allowance and must be compared against
    /// half the Kindergeld. Comparing against the full amount would wrongly favour Kindergeld
    /// for every such parent.
    #[test]
    fn a_half_allowance_is_compared_against_half_the_child_benefit() {
        let law = law();
        let half = assess(
            &income_of(200_000),
            FilingStatus::Individual,
            None,
            5,
            Money::ZERO,
            &law,
        )
        .expect("assesses");

        let ChildRelief::Allowance {
            deducted,
            clawed_back,
            ..
        } = half.child_relief
        else {
            panic!("the allowance should win at 200 000 EUR");
        };
        assert_eq!(
            deducted,
            law.deductions
                .child_allowance_total()
                .unwrap()
                .div_int(2, casivell_core::Rounding::Floor)
                .unwrap()
        );
        assert_eq!(
            clawed_back,
            law.deductions
                .child_benefit_annual()
                .unwrap()
                .div_int(2, casivell_core::Rounding::Floor)
                .unwrap()
        );
    }

    // ---------------------------------------------------------------------
    // The surcharges and the § 51a base
    // ---------------------------------------------------------------------

    /// § 51a Abs. 2 requires the surcharge base to be computed *with* the Kinderfreibetrag,
    /// whatever the Günstigerprüfung decided about the income tax. This is the correction
    /// `casivell_tax::church_tax` records as missing from the annual path — and it is now
    /// applied here.
    #[test]
    fn the_surcharges_use_the_child_reduced_base() {
        let law = law();
        // 35 000 EUR, assessed individually: the marginal rate there is about 30 %, so the
        // allowance is worth less than the 3 108 EUR of Kindergeld and the Günstigerprüfung
        // keeps the benefit. The income tax is therefore unreduced — but the surcharges must
        // still be levied on the child-reduced base.
        //
        // The crossover for an individual sits near 40 700 EUR, where the marginal rate
        // reaches 3 108 / 9 756 = 31.9 %. It is far higher for a couple, because a joint
        // income is halved before the tariff is applied.
        let with_child = assess(
            &income_of(35_000),
            FilingStatus::Individual,
            Some(Bundesland::NordrheinWestfalen),
            10,
            Money::ZERO,
            &law,
        )
        .expect("assesses");
        let childless = assess(
            &income_of(35_000),
            FilingStatus::Individual,
            Some(Bundesland::NordrheinWestfalen),
            0,
            Money::ZERO,
            &law,
        )
        .expect("assesses");

        assert!(
            matches!(with_child.child_relief, ChildRelief::ChildBenefit { .. }),
            "Kindergeld should win at 35 000 EUR assessed individually"
        );
        assert_eq!(
            with_child.income_tax, childless.income_tax,
            "the income tax should be unreduced when Kindergeld wins"
        );
        assert!(
            with_child.church_tax < childless.church_tax,
            "but the church tax must be levied on the child-reduced base"
        );
    }

    #[test]
    fn church_tax_is_only_levied_on_a_member() {
        let law = law();
        let secular = assess(
            &income_of(60_000),
            FilingStatus::Individual,
            None,
            0,
            Money::ZERO,
            &law,
        )
        .expect("assesses");
        assert_eq!(secular.church_tax, Money::ZERO);

        let bavarian = assess(
            &income_of(60_000),
            FilingStatus::Individual,
            Some(Bundesland::Bayern),
            0,
            Money::ZERO,
            &law,
        )
        .expect("assesses");
        let rhenish = assess(
            &income_of(60_000),
            FilingStatus::Individual,
            Some(Bundesland::NordrheinWestfalen),
            0,
            Money::ZERO,
            &law,
        )
        .expect("assesses");
        // 8 % in Bavaria against 9 % elsewhere.
        assert!(bavarian.church_tax < rhenish.church_tax);
    }

    // ---------------------------------------------------------------------
    // The refund
    // ---------------------------------------------------------------------

    /// The refund is withheld less owed, in both directions.
    #[test]
    fn the_refund_is_withheld_less_owed() {
        let law = law();
        let income = income_of(50_000);
        let owed = assess(
            &income,
            FilingStatus::Individual,
            None,
            0,
            Money::ZERO,
            &law,
        )
        .expect("assesses")
        .total_liability;

        let over = assess(
            &income,
            FilingStatus::Individual,
            None,
            0,
            owed.add(euro(500)).unwrap(),
            &law,
        )
        .expect("assesses");
        assert_eq!(over.refund, euro(500));

        let under = assess(
            &income,
            FilingStatus::Individual,
            None,
            0,
            owed.sub(euro(300)).unwrap(),
            &law,
        )
        .expect("assesses");
        assert_eq!(under.refund, euro(-300));
    }

    /// No assessment from this crate claims to be exact, because § 10's interaction has not
    /// been reconciled against a real Steuerbescheid. The flag exists so a caller cannot
    /// present the figure as a liability by accident.
    #[test]
    fn no_assessment_claims_to_be_exact() {
        for einkommen in [0_i64, 30_000, 80_000, 250_000] {
            let assessment = assess(
                &income_of(einkommen),
                FilingStatus::Individual,
                None,
                10,
                Money::ZERO,
                &law(),
            )
            .expect("assesses");
            assert!(
                !assessment.is_exact,
                "an assessment at {einkommen} claimed to be exact"
            );
        }
    }

    /// The total liability is the sum of its parts, and monotonic in income.
    #[test]
    fn the_liability_is_consistent_and_monotonic() {
        let law = law();
        let mut previous = Money::ZERO;
        let mut einkommen = 0_i64;
        while einkommen <= 300_000 {
            let a = assess(
                &income_of(einkommen),
                FilingStatus::Individual,
                Some(Bundesland::Berlin),
                0,
                Money::ZERO,
                &law,
            )
            .expect("assesses");

            assert_eq!(
                a.total_liability,
                a.income_tax
                    .add(a.solidarity_surcharge)
                    .unwrap()
                    .add(a.church_tax)
                    .unwrap(),
                "the parts do not sum at {einkommen}"
            );
            assert!(
                a.total_liability >= previous,
                "the liability fell at {einkommen}"
            );
            previous = a.total_liability;
            einkommen = einkommen.saturating_add(3_100);
        }
    }
}
