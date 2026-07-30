//! Cross-checking the real § 10 deduction against the Programmablaufplan's own allowance.
//!
//! # What this test is for
//!
//! `casivell-income` has no official reference table — the reason is set out in the crate
//! documentation. What it *does* have is a second, independently verified computation of
//! nearly the same quantity: the Vorsorgepauschale of § 39b Abs. 2 Satz 5 Nr. 3, which
//! `casivell-payroll` implements and which is checked against 516 published values.
//!
//! The two are not meant to be equal. The Vorsorgepauschale is a deliberate simplification
//! for payroll, and § 10 is the real thing. But they are computed from the same
//! contributions, so the *relationship* between them is a real constraint on both, and a
//! defect in either would show up as the relationship breaking.
//!
//! # The expected relationship
//!
//! Three differences, all in the same direction:
//!
//! - The Vorsorgepauschale uses **7.0 %** for health — half the *reduced* GKV rate — while
//!   § 10 uses the employee's actual 7.3 % less the 4 % Krankengeld reduction, so 7.008 %.
//!   Nearly identical, and slightly in § 10's favour.
//! - § 10 deducts the **whole** health and care contribution via the Abs. 4 Satz 4 override.
//!   The Vorsorgepauschale caps the unemployment-plus-health basket at 1 900 € when that is
//!   the larger of its two candidates, but takes the uncapped health figure otherwise — so
//!   the two agree closely here for a mid earner.
//! - The Vorsorgepauschale includes **unemployment insurance** in its capped alternative;
//!   § 10 effectively deducts nothing for it, because the override has already passed the cap.
//!
//! Net: the two land within a few percent of each other for an ordinary employee, and § 10
//! should be the larger for anyone whose fund charges a supplementary rate — because the
//! Vorsorgepauschale halves the Zusatzbeitrag but § 10 deducts it in full.
//!
//! A divergence beyond that band means one of the two is wrong, and the test says which
//! figures to look at.

// An integration test is its own crate, so the library root's `cfg(test)` exemption does not
// reach it. See docs/CODING_STANDARD.md R7.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_core::{Money, Rate, TaxYear};
use casivell_income::vorsorge::Contributions;
use casivell_income::{Employee, taxable_income};
use casivell_lawdata::{Bundesland, DeductionParameters, SocialParameters, TaxClass};
use casivell_payroll::{Employment, HealthCover, PayPeriod, PayrollLaw, withhold};
use casivell_social::{Insured, contributions};

fn year() -> TaxYear {
    TaxYear::new(2026).expect("2026 is verified")
}

fn supplementary() -> Rate {
    Rate::from_percent_millis(2_900).expect("valid rate")
}

/// The § 10 deduction and the Vorsorgepauschale for the same employee on the same salary.
fn both(monthly_gross_euro: i64, children: u8, is_parent: bool) -> (Money, Money) {
    let gross = Money::from_euro(monthly_gross_euro).expect("valid");
    let social = SocialParameters::for_year(year()).expect("enacted");
    let deductions = DeductionParameters::for_year(year()).expect("enacted");
    let law = PayrollLaw::for_year(year()).expect("enacted");

    let insured = Insured::new(
        40,
        is_parent,
        children,
        Bundesland::NordrheinWestfalen,
        Some(supplementary()),
    )
    .expect("valid profile");

    // The real § 10 deduction, from the actual contributions.
    let split = contributions(gross, &social, &insured).expect("computes");
    let paid = Contributions::from_social(&split, &social, supplementary(), 12).expect("builds");
    let employee = Employee {
        gross_annual: gross.mul_int(12).expect("valid"),
        work_expenses: Money::ZERO,
        contributions: paid,
        church_tax_paid: Money::ZERO,
        other_special_expenses: Money::ZERO,
        children,
    };
    let statutory = taxable_income(&employee, &deductions)
        .expect("computes")
        .provision
        .total;

    // The Vorsorgepauschale the same employee's payroll would use.
    let employment = Employment::new(
        insured,
        TaxClass::Class1,
        0,
        HealthCover::Statutory {
            supplementary_rate: supplementary(),
        },
        None,
    )
    .expect("valid employment");
    let pauschale = withhold(gross, PayPeriod::Month, &employment, &law)
        .expect("withholds")
        .vorsorgepauschale;

    (statutory, pauschale)
}

/// The two must stay within a few percent of each other across the salary range. A larger gap
/// means a defect in one of them, and both are computed from the same contributions so the
/// comparison is meaningful rather than coincidental.
#[test]
fn the_real_deduction_tracks_the_vorsorgepauschale() {
    let mut monthly = 2_000_i64;
    while monthly <= 9_000 {
        let (statutory, pauschale) = both(monthly, 0, false);

        let difference = statutory
            .sub(pauschale)
            .expect("representable")
            .cents()
            .abs();
        let allowed = pauschale.cents() / 10;
        assert!(
            difference <= allowed,
            "at {monthly} EUR/month the § 10 deduction is {} and the Vorsorgepauschale {}, \
             a gap of {difference} cents beyond the 10 % band",
            statutory.cents(),
            pauschale.cents(),
        );
        monthly = monthly.saturating_add(500);
    }
}

/// § 10 should be the *larger* of the two for an employee whose fund charges a supplementary
/// rate, because the Vorsorgepauschale halves the Zusatzbeitrag while § 10 deducts it in full.
///
/// The direction is the informative part: a simplification that came out systematically
/// generous would mean payroll over-deducted, which is the opposite of how the statute is
/// designed.
#[test]
fn the_real_deduction_is_the_more_generous_one() {
    for monthly in [2_500_i64, 4_000, 5_500, 7_000] {
        let (statutory, pauschale) = both(monthly, 0, false);
        assert!(
            statutory >= pauschale,
            "at {monthly} EUR/month § 10 gave {} but the Vorsorgepauschale gave {}",
            statutory.cents(),
            pauschale.cents(),
        );
    }
}

/// Both must respond to the care-insurance circumstances in the same direction: a childless
/// employee pays more and therefore deducts more, under either computation.
#[test]
fn both_computations_track_the_childless_surcharge() {
    let (childless_statutory, childless_pauschale) = both(4_500, 0, false);
    let (parent_statutory, parent_pauschale) = both(4_500, 1, true);

    assert!(
        childless_statutory > parent_statutory,
        "§ 10 should deduct more for a childless employee"
    );
    assert!(
        childless_pauschale > parent_pauschale,
        "the Vorsorgepauschale should too"
    );
}

/// Above the contribution ceilings both must stop growing, since both are computed on capped
/// contributions.
#[test]
fn both_saturate_above_the_contribution_ceilings() {
    let (at_ceiling_statutory, at_ceiling_pauschale) = both(8_450, 0, false);
    let (far_above_statutory, far_above_pauschale) = both(15_000, 0, false);

    assert_eq!(at_ceiling_statutory, far_above_statutory);
    assert_eq!(at_ceiling_pauschale, far_above_pauschale);
}
