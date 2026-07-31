//! Elterngeld against the figures published about it.
//!
//! # The verification problem, and what is available
//!
//! Elterngeld has no official reference table. It is defined as a calculation, not as amounts,
//! so there is nothing to transcribe and nothing to check against in the way
//! `casivell-payroll` is checked against 516 BMF values.
//!
//! What is available, in descending order of strength:
//!
//! 1. **The statute's own stated boundaries.** § 2 Abs. 2 names the rates and the thresholds
//!    outright, so the sliding scale is fully checkable against the text. Those live in the
//!    unit tests, which pin all six boundary values exactly.
//! 2. **A derived boundary that published commentary states independently.** Every Elterngeld
//!    guide says the maximum takes over "ab etwa 2 770 Euro Elterngeld-Netto". That figure is
//!    not in § 2 Abs. 1 — it follows from 1 800 ÷ 65 %, and it is the same 2 770 € that § 2
//!    Abs. 3 Satz 2 names as the difference cap. Two provisions and the commentary agreeing on
//!    one number is a real check, and this file makes it.
//! 3. **The largest deduction runs through verified code.** § 2e computes the tax with the
//!    Programmablaufplan, so that part of the stylised net inherits the 516-value check.
//! 4. **A bounded comparison against published commentary** for the tax-class effect, which is
//!    the property Elterngeld guides most often quantify.
//!
//! What is *not* available is an end-to-end official figure. Nothing here should be presented
//! as an entitlement; it is a good estimate of one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_benefits::{Elterngeld, ElterngeldRequest, elterngeld};
use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, DeductionParameters, ElterngeldParameters, TaxClass};
use casivell_payroll::{Employment, HealthCover, PayrollLaw};
use casivell_social::Insured;

fn euro(amount: i64) -> Money {
    Money::from_euro(amount).unwrap()
}

fn year() -> TaxYear {
    TaxYear::new(2026).unwrap()
}

fn at(gross: i64, class: TaxClass) -> Elterngeld {
    let insured = Insured::new(30, true, 1, Bundesland::NordrheinWestfalen, None).unwrap();
    let employment = Employment::new(
        insured,
        class,
        0,
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        },
        None,
    )
    .unwrap();
    elterngeld(
        &ElterngeldRequest::full_interruption(euro(gross), euro(50_000)),
        &employment,
        &PayrollLaw::for_year(year()).unwrap(),
        &DeductionParameters::for_year(year()).unwrap(),
        &ElterngeldParameters::for_year(year()).unwrap(),
    )
    .unwrap()
}

/// The 2 770 € boundary, which three independent things must agree on.
///
/// Published commentary says the maximum takes over "ab etwa 2 770 Euro Elterngeld-Netto".
/// That is not a figure § 2 Abs. 1 states — it is 1 800 ÷ 65 % = 2 769,23, rounded up. And it
/// is separately the exact cap § 2 Abs. 3 Satz 2 names for the difference calculation.
///
/// So: the statutory maximum, the statutory difference cap, and the commentary all land on the
/// same number, by three different routes. If this engine's stylised net were wrong, the point
/// at which its benefit stops rising would not sit there.
#[test]
fn the_maximum_takes_over_at_the_published_elterngeld_netto() {
    let beeg = ElterngeldParameters::for_year(year()).unwrap();

    // Walk up in euro steps and find where the amount first reaches the maximum.
    let mut crossover: Option<Elterngeld> = None;
    let mut last_below = None;
    for gross in 4_000..6_000 {
        let result = at(gross, TaxClass::Class1);
        if result.monthly_amount >= beeg.maximum_monthly {
            crossover = Some(result);
            break;
        }
        last_below = Some(result);
    }

    let crossed = crossover.expect("the maximum should be reached below 6 000 EUR gross");
    let below = last_below.expect("and not at the very first step");

    // At the crossover the stylised net has just passed 1 800 / 65 % = 2 769,23.
    assert!(
        crossed.net_before > euro(2_769),
        "the maximum should not bind below 2 769 EUR of stylised net, but did at {:?}",
        crossed.net_before
    );
    assert!(
        below.net_before < euro(2_775),
        "the maximum should already bind by 2 775 EUR of stylised net, but had not at {:?}",
        below.net_before
    );

    // And the same figure is the statutory difference cap, to the euro.
    assert_eq!(beeg.difference_income_cap, euro(2_770));
    // 2 770 x 65 % = 1 800,50, which the maximum then trims to 1 800 — the fifty cents that
    // make the two provisions agree rather than merely sit near each other.
    assert_eq!(
        beeg.difference_income_cap
            .mul_rate(beeg.floor_rate, casivell_core::Rounding::HalfUp)
            .unwrap(),
        euro(1_800).add(Money::from_cents(50).unwrap()).unwrap()
    );
}

/// Above the crossover, more salary buys nothing at all.
#[test]
fn beyond_the_cap_more_salary_changes_nothing() {
    let beeg = ElterngeldParameters::for_year(year()).unwrap();
    for gross in [6_000_i64, 9_000, 15_000, 30_000] {
        assert_eq!(
            at(gross, TaxClass::Class1).monthly_amount,
            beeg.maximum_monthly
        );
    }
}

/// The tax class is the largest single lever on Elterngeld, and the one every guide points at.
///
/// It works because § 2e deducts tax at the class in force *before* the birth: class III has
/// the lowest withholding, so the highest stylised net, so the largest benefit — even though
/// the couple's real annual tax is settled identically by the assessment either way. The
/// benefit is computed from withholding, and withholding is not the tax.
///
/// Published commentary puts the spread at 3 000 € gross at up to about 450 € a month. This
/// asserts a *band* rather than a point, because the exact figure depends on the church tax,
/// the fund's Zusatzbeitrag and the Bundesland, none of which the commentary fixes.
#[test]
fn the_tax_class_spread_matches_published_commentary() {
    let three = at(3_000, TaxClass::Class3).monthly_amount;
    let one = at(3_000, TaxClass::Class1).monthly_amount;
    let five = at(3_000, TaxClass::Class5).monthly_amount;

    assert!(
        three > one && one > five,
        "III > I > V, as the withholding is"
    );

    let spread = three.sub(five).unwrap();
    assert!(
        spread > euro(300) && spread < euro(500),
        "the class III/V spread at 3 000 EUR came out at {spread:?}, outside the band \
         published commentary puts at roughly 450 EUR"
    );
}

/// Sanity: the benefit must be a plausible fraction of the salary it replaces, across the
/// whole range. Loose bounds deliberately — this catches an order-of-magnitude error or a
/// misplaced deduction, not a rounding difference.
#[test]
fn the_benefit_is_a_plausible_share_of_the_salary() {
    let beeg = ElterngeldParameters::for_year(year()).unwrap();
    for gross in [1_500_i64, 2_000, 2_500, 3_000, 3_500, 4_000] {
        let result = at(gross, TaxClass::Class1);
        let share = result.monthly_amount.cents() * 100 / euro(gross).cents();
        assert!(
            (35..=60).contains(&share),
            "at {gross} EUR gross the benefit was {share} % of it, which is not plausible"
        );
        // And it is always between the statutory bounds.
        assert!(result.monthly_amount >= beeg.minimum_monthly);
        assert!(result.monthly_amount <= beeg.maximum_monthly);
    }
}
