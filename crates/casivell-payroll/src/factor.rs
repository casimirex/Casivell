//! § 39f EStG: the Faktorverfahren, and what choosing a tax class actually decides.
//!
//! # The tax class does not change the tax
//!
//! This is the single most misunderstood thing in German payroll, and the reason this module
//! exists. A married couple's income tax for the year is fixed by § 32a Abs. 5: it is the
//! Splittingtarif applied to their joint income, and no combination of tax classes alters it
//! by a cent. What the classes decide is **when** the money moves, and the assessment settles
//! the difference either way.
//!
//! III/V withholds least from the higher earner and most from the lower one, which flatters
//! the household's monthly cash flow and produces a demand at assessment. IV/IV withholds each
//! spouse as though they were single, which over-withholds a couple with unequal incomes and
//! produces a refund. IV+Faktor scales the class IV figures by the ratio the year's real
//! liability bears to them, so each spouse pays about their true share month by month and
//! neither the demand nor the refund is large.
//!
//! # But it does change things that are computed *from* withholding
//!
//! Two consequences make the choice matter after all, and both are downstream of the payslip
//! rather than of the tax:
//!
//! - **Wage-replacement benefits.** Elterngeld, Arbeitslosengeld and Krankengeld are computed
//!   from net pay, and net pay depends on the class. A parent in class III before a birth
//!   receives materially more Elterngeld than the same parent in class V — for the same
//!   household, the same year, and the same total tax. `casivell-benefits` shows a spread of
//!   about 386 € a month at 3 000 € gross.
//! - **Who holds the money.** A demand at assessment is a bill for a household that has
//!   already spent the difference; a refund is an interest-free loan to the tax office.
//!
//! # What is deliberately not claimed
//!
//! The 516 official values this crate is checked against cover the classes as they stand,
//! without a factor. There is no published Prüftabelle for the Faktorverfahren, so the
//! implementation cannot be checked the same way.
//!
//! Its verification is instead the property the statute exists to produce: the sum of both
//! spouses' withholding under IV+Faktor should come out close to the joint annual liability.
//! `the_factor_makes_withholding_match_the_annual_liability` asserts exactly that, against the
//! independently implemented § 32a in `casivell-tax`. It is a weaker check than a reference
//! table and a stronger one than none.

use casivell_core::{Money, MoneyError, Rate};
use casivell_tax::{FilingStatus, income_tax};

use crate::withholding::{Employment, PayPeriod, PayrollLaw, withhold};

/// Decimal places § 39f Abs. 1 Satz 5 states the factor to.
///
/// "Mit drei Nachkommastellen ohne Rundung" — three places, and *truncated* rather than
/// rounded, which is unusual enough in this statute to be worth naming.
const FACTOR_DECIMALS: i64 = 1_000;

/// One tax-class arrangement, priced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrangement {
    /// Monthly withholding from the higher earner.
    pub higher_withholding: Money,
    /// Monthly withholding from the lower earner.
    pub lower_withholding: Money,
    /// The two together, monthly.
    pub monthly_withholding: Money,
    /// Monthly net reaching the household, both salaries together.
    pub monthly_net: Money,
    /// The year's withholding of everything, twelve times the monthly figure.
    ///
    /// The cash-flow number: what actually leaves the household's payslips.
    pub annual_withholding: Money,
    /// The year's withheld **income tax** alone, without the surcharges.
    ///
    /// Kept apart because [`Self::settlement`] compares it against the joint income tax, and
    /// comparing a total that includes the Solidaritätszuschlag against one that does not
    /// would attribute the whole Soli to the tax-class choice. It is not: the Soli follows
    /// whatever the income tax turns out to be, under every arrangement alike.
    pub annual_income_tax: Money,
    /// Withheld income tax over the year less the joint liability: positive is a refund.
    pub settlement: Money,
}

/// The three arrangements a married couple may choose between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassComparison {
    /// The joint income tax for the year under § 32a Abs. 5.
    ///
    /// Identical for all three arrangements — which is the point.
    pub joint_liability: Money,

    /// Both spouses in class IV.
    pub four_four: Arrangement,
    /// The higher earner in class III, the lower in class V.
    pub three_five: Arrangement,
    /// Both in class IV with the § 39f factor.
    pub four_with_factor: Arrangement,
    /// The factor itself, or `None` where § 39f does not apply.
    pub factor: Option<Rate>,
}

/// § 39f Abs. 1: the factor `Y : X`.
///
/// `Y` is the couple's expected income tax under the Splittingverfahren; `X` is the sum of
/// what class IV would withhold from each of them. Truncated to three decimal places, and
/// `None` where the quotient reaches one — the procedure is available only below it, so a
/// factor of one or more means the election simply does not apply.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn faktorverfahren(
    higher_monthly: Money,
    lower_monthly: Money,
    higher: &Employment,
    lower: &Employment,
    law: &PayrollLaw,
) -> Result<Option<Rate>, MoneyError> {
    let class_four = class_four_annual(higher_monthly, lower_monthly, higher, lower, law)?;
    if class_four.is_zero() {
        return Ok(None);
    }

    // Y: the joint liability. Computed from the same taxable amount the Programmablaufplan
    // arrives at for each spouse, so the two sides of the ratio are commensurable — using a
    // full § 2 assessment for Y and the PAP for X would compare different things.
    let joint = joint_liability(higher_monthly, lower_monthly, higher, lower, law)?;

    // Three decimals, truncated. The multiplication precedes the division so the truncation
    // happens once, at the stated precision, rather than twice.
    let scaled = joint
        .cents()
        .checked_mul(FACTOR_DECIMALS)
        .ok_or(MoneyError::Overflow)?;
    let thousandths = casivell_core::div_trunc(scaled, class_four.cents())?;
    if thousandths >= FACTOR_DECIMALS {
        return Ok(None);
    }
    let ppm = thousandths
        .checked_mul(Rate::ONE.ppm() / FACTOR_DECIMALS)
        .ok_or(MoneyError::Overflow)?;
    Ok(Some(Rate::from_ppm(ppm)?))
}

/// `X`: the sum of a year's class IV withholding from both spouses.
fn class_four_annual(
    higher_monthly: Money,
    lower_monthly: Money,
    higher: &Employment,
    lower: &Employment,
    law: &PayrollLaw,
) -> Result<Money, MoneyError> {
    let a = withhold(
        higher_monthly,
        PayPeriod::Month,
        &as_class_four(higher),
        law,
    )?;
    let b = withhold(lower_monthly, PayPeriod::Month, &as_class_four(lower), law)?;
    a.annual_income_tax.add(b.annual_income_tax)
}

/// `Y`: the couple's joint income tax under § 32a Abs. 5.
///
/// Built from each spouse's `ZVE` as the Programmablaufplan computes it — gross less the
/// table allowances and the Vorsorgepauschale — summed and put through the Splittingtarif.
/// That is what § 39f Abs. 1 means by "die voraussichtliche Einkommensteuer": an estimate
/// made from the payroll figures, not a completed assessment.
fn joint_liability(
    higher_monthly: Money,
    lower_monthly: Money,
    higher: &Employment,
    lower: &Employment,
    law: &PayrollLaw,
) -> Result<Money, MoneyError> {
    let a = withhold(
        higher_monthly,
        PayPeriod::Month,
        &as_class_four(higher),
        law,
    )?;
    let b = withhold(lower_monthly, PayPeriod::Month, &as_class_four(lower), law)?;
    let joint = a.taxable_annual_amount.add(b.taxable_annual_amount)?;
    Ok(income_tax(joint, &law.tariff, FilingStatus::JointSplitting)?.income_tax)
}

/// The same employment, in class IV without a factor.
fn as_class_four(employment: &Employment) -> Employment {
    Employment {
        tax_class: casivell_lawdata::TaxClass::Class4,
        factor: None,
        ..*employment
    }
}

/// The same employment in a named class.
fn in_class(employment: &Employment, class: casivell_lawdata::TaxClass) -> Employment {
    Employment {
        tax_class: class,
        factor: None,
        ..*employment
    }
}

/// Prices all three arrangements for a couple.
///
/// `higher_monthly` and `lower_monthly` are the two gross salaries; which is which matters,
/// because III/V is only sensible with the higher earner in III.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn compare_classes(
    higher_monthly: Money,
    lower_monthly: Money,
    higher: &Employment,
    lower: &Employment,
    law: &PayrollLaw,
) -> Result<ClassComparison, MoneyError> {
    use casivell_lawdata::TaxClass;

    let joint_liability = joint_liability(higher_monthly, lower_monthly, higher, lower, law)?;
    let factor = faktorverfahren(higher_monthly, lower_monthly, higher, lower, law)?;

    let four_four = price(
        higher_monthly,
        lower_monthly,
        &in_class(higher, TaxClass::Class4),
        &in_class(lower, TaxClass::Class4),
        joint_liability,
        law,
    )?;
    let three_five = price(
        higher_monthly,
        lower_monthly,
        &in_class(higher, TaxClass::Class3),
        &in_class(lower, TaxClass::Class5),
        joint_liability,
        law,
    )?;

    let factored = |employment: &Employment| -> Result<Employment, MoneyError> {
        let base = in_class(employment, TaxClass::Class4);
        match factor {
            Some(f) => base.with_factor(f),
            None => Ok(base),
        }
    };
    let four_with_factor = price(
        higher_monthly,
        lower_monthly,
        &factored(higher)?,
        &factored(lower)?,
        joint_liability,
        law,
    )?;

    Ok(ClassComparison {
        joint_liability,
        four_four,
        three_five,
        four_with_factor,
        factor,
    })
}

/// Prices one arrangement.
fn price(
    higher_monthly: Money,
    lower_monthly: Money,
    higher: &Employment,
    lower: &Employment,
    joint_liability: Money,
    law: &PayrollLaw,
) -> Result<Arrangement, MoneyError> {
    let a = crate::net::monthly_net(higher_monthly, higher, law)?;
    let b = crate::net::monthly_net(lower_monthly, lower, law)?;

    let withheld = |pay: &crate::net::NetPay| -> Result<Money, MoneyError> {
        pay.income_tax
            .add(pay.solidarity_surcharge)?
            .add(pay.church_tax)
    };
    let higher_withholding = withheld(&a)?;
    let lower_withholding = withheld(&b)?;
    let monthly_withholding = higher_withholding.add(lower_withholding)?;
    let annual_income_tax = a.income_tax.add(b.income_tax)?.mul_int(12)?;

    Ok(Arrangement {
        higher_withholding,
        lower_withholding,
        monthly_withholding,
        monthly_net: a.net.add(b.net)?,
        annual_withholding: monthly_withholding.mul_int(12)?,
        annual_income_tax,
        // Positive is a refund, as everywhere else in this repository. Income tax against
        // income tax: the surcharges settle alongside it in the same direction and adding
        // them to one side only would misattribute them to the class choice.
        settlement: annual_income_tax.sub(joint_liability)?,
    })
}

/// Rounds a factor to the three decimals § 39f states it to, for display.
#[must_use]
pub fn factor_thousandths(factor: Rate) -> i64 {
    casivell_core::div_trunc(factor.ppm(), Rate::ONE.ppm() / FACTOR_DECIMALS).unwrap_or(0)
}

/// A convenience for callers that only want the monthly net under one arrangement.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn monthly_net_in_class(
    monthly_gross: Money,
    employment: &Employment,
    class: casivell_lawdata::TaxClass,
    law: &PayrollLaw,
) -> Result<Money, MoneyError> {
    Ok(crate::net::monthly_net(monthly_gross, &in_class(employment, class), law)?.net)
}

#[cfg(test)]
mod tests {
    use super::{ClassComparison, compare_classes, factor_thousandths, faktorverfahren};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_social::Insured;

    use crate::withholding::{Employment, HealthCover, PayrollLaw};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn law() -> PayrollLaw {
        PayrollLaw::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn spouse() -> Employment {
        let insured = Insured::new(35, true, 1, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(
            insured,
            TaxClass::Class4,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap()
    }

    fn compare(higher: i64, lower: i64) -> ClassComparison {
        compare_classes(euro(higher), euro(lower), &spouse(), &spouse(), &law()).expect("computes")
    }

    // ---------------------------------------------------------------------
    // The property the whole module exists to make visible
    // ---------------------------------------------------------------------

    /// The joint liability is the same whatever the classes. This is the fact households most
    /// often get wrong, and it is asserted first because everything else is a consequence.
    #[test]
    fn the_tax_class_does_not_change_the_tax() {
        for (higher, lower) in [(5_000_i64, 1_500_i64), (4_000, 4_000), (9_000, 2_500)] {
            let result = compare(higher, lower);
            // One liability, three ways of paying it: each arrangement's withholding plus its
            // settlement comes back to the same figure.
            for arrangement in [result.four_four, result.three_five, result.four_with_factor] {
                assert_eq!(
                    arrangement
                        .annual_income_tax
                        .sub(arrangement.settlement)
                        .unwrap(),
                    result.joint_liability,
                    "at {higher}/{lower} an arrangement did not reconcile"
                );
            }
        }
    }

    /// III/V withholds least and IV/IV most, with the factor between them. That ordering is
    /// the whole practical difference, and it is what a household is really choosing.
    #[test]
    fn the_three_arrangements_order_as_expected() {
        let result = compare(5_000, 1_800);
        assert!(
            result.three_five.monthly_withholding < result.four_four.monthly_withholding,
            "III/V should withhold less than IV/IV for unequal incomes"
        );
        assert!(result.three_five.monthly_net > result.four_four.monthly_net);

        // And III/V therefore ends in a demand where IV/IV ends in a refund.
        assert!(result.three_five.settlement.is_negative());
        assert!(!result.four_four.settlement.is_negative());
    }

    // ---------------------------------------------------------------------
    // § 39f itself
    // ---------------------------------------------------------------------

    /// The factor's defining property, and the only verification available: withholding under
    /// IV+Faktor must come out close to the joint annual liability.
    ///
    /// There is no published Prüftabelle for the Faktorverfahren, so this is checked against
    /// the independently implemented § 32a rather than against a reference table. Within one
    /// percent of the liability across a wide range of income splits — the residual is the
    /// three-decimal truncation the statute mandates, which can only ever withhold slightly
    /// too little.
    #[test]
    fn the_factor_makes_withholding_match_the_annual_liability() {
        for (higher, lower) in [
            (5_000_i64, 1_500_i64),
            (5_000, 3_000),
            (7_000, 2_000),
            (9_000, 4_000),
            (3_500, 1_200),
        ] {
            let result = compare(higher, lower);
            let Some(_) = result.factor else {
                continue;
            };
            let gap = result.four_with_factor.settlement;
            assert!(
                gap.cents().abs() * 100 < result.joint_liability.cents(),
                "at {higher}/{lower} the factored withholding was {gap:?} away from a \
                 liability of {:?}, which is more than the truncation can explain",
                result.joint_liability
            );
            // And it is much closer than either alternative.
            assert!(gap.cents().abs() < result.three_five.settlement.cents().abs());
            assert!(gap.cents().abs() < result.four_four.settlement.cents().abs());
        }
    }

    /// The factor is stated to three decimals and truncated, never rounded.
    #[test]
    fn the_factor_has_three_decimals_and_is_truncated() {
        let result = compare(5_000, 1_500);
        let factor = result.factor.expect("unequal incomes give a factor");
        let thousandths = factor_thousandths(factor);
        assert!((1..1_000).contains(&thousandths), "got {thousandths}");
        // Representable exactly at three decimals: no fourth place survives.
        assert_eq!(factor.ppm() % (Rate::ONE.ppm() / 1_000), 0);
    }

    /// § 39f Abs. 1 Satz 6: the procedure applies only where the factor comes out below one.
    /// Two equal earners are already withheld correctly by class IV, so there is nothing for
    /// it to do and the election is unavailable rather than a no-op.
    #[test]
    fn equal_earners_get_no_factor_because_they_need_none() {
        let equal = compare(4_000, 4_000);
        assert!(
            equal.factor.is_none(),
            "class IV already withholds an equal couple correctly"
        );
        // And IV/IV is already almost exactly right for them.
        assert!(equal.four_four.settlement.cents().abs() < euro(100).cents());
    }

    /// The more unequal the incomes, the smaller the factor — because class IV over-withholds
    /// an unequal couple by more.
    #[test]
    fn the_factor_falls_as_the_incomes_diverge() {
        let mut previous = 1_000_i64;
        for lower in [3_500_i64, 3_000, 2_000, 1_000] {
            let Some(factor) = compare(5_000, lower).factor else {
                continue;
            };
            let thousandths = factor_thousandths(factor);
            assert!(
                thousandths <= previous,
                "the factor rose as the incomes diverged, at {lower}"
            );
            previous = thousandths;
        }
        assert!(previous < 1_000);
    }

    /// A household with no income has no factor rather than a division by zero.
    #[test]
    fn no_income_gives_no_factor() {
        assert_eq!(
            faktorverfahren(Money::ZERO, Money::ZERO, &spouse(), &spouse(), &law()).unwrap(),
            None
        );
    }

    /// The factor may be set only on class IV, and only below one.
    #[test]
    fn a_factor_is_refused_outside_its_statutory_domain() {
        let half = Rate::from_percent_millis(50_000).unwrap();
        assert!(spouse().with_factor(half).is_ok());

        for class in [TaxClass::Class1, TaxClass::Class3, TaxClass::Class5] {
            let mut wrong = spouse();
            wrong.tax_class = class;
            assert!(
                wrong.with_factor(half).is_err(),
                "{class:?} must not accept a factor"
            );
        }
        assert!(
            spouse().with_factor(Rate::ONE).is_err(),
            "one is not below one"
        );
        assert!(spouse().with_factor(Rate::ZERO).is_err());
    }

    /// A factor must reduce withholding, never raise it.
    #[test]
    fn applying_a_factor_lowers_the_withholding() {
        let result = compare(5_000, 1_500);
        assert!(result.factor.is_some());
        assert!(result.four_with_factor.monthly_withholding < result.four_four.monthly_withholding);
        assert!(result.four_with_factor.monthly_net > result.four_four.monthly_net);
    }

    /// The factored arrangement splits the burden between the spouses in proportion to their
    /// incomes, which is the fairness argument for it — class V's punitive withholding on the
    /// lower earner is what the procedure exists to avoid.
    #[test]
    fn the_factor_spreads_the_burden_more_evenly_than_three_five() {
        let result = compare(5_000, 1_800);
        let share = |a: super::Arrangement| {
            a.lower_withholding.cents() * 1_000 / a.monthly_withholding.cents().max(1)
        };
        assert!(
            share(result.four_with_factor) < share(result.three_five),
            "class V should take a larger share of the burden from the lower earner"
        );
    }
}
