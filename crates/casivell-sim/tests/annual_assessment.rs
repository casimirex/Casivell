//! The annual assessment, as the kernel runs it.
//!
//! # The cross-check this file exists for
//!
//! `casivell-income` was the least verified crate in the repository: § 10's interaction has
//! no official reference table, and its own documentation says so. Running it inside the
//! kernel produces the check that was missing.
//!
//! Lohnsteuer withholding is *designed* to be right for an employee whose year is flat — that
//! is the whole premise of § 39b, which annualises each month and divides back. So for a flat
//! year the assessment must return almost nothing. It does:
//! `withholding_and_the_annual_assessment_agree_on_a_flat_year` finds the two paths within a
//! few euro of each other across a sixfold range of salaries.
//!
//! That is not a tautology. The two are computed from different statutes by different code:
//! withholding runs the BMF Programmablaufplan with its deliberately simplified
//! Vorsorgepauschale, and the assessment runs § 2 EStG with the real § 10 deduction. Nothing
//! makes them agree except both being right. It is the strongest evidence available that the
//! § 10 chain is correct, short of a real Steuerbescheid.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::{Employment, HealthCover};
use casivell_sim::{
    Basis, Event, Horizon, Household, MonthSnapshot, NoAssessment, SETTLEMENT_LAG_MONTHS, Schedule,
    SimulationConfig, Sink, Summary, filing_status_for, simulate,
};
use casivell_social::Insured;

struct Collect(Vec<MonthSnapshot>);

impl Sink for Collect {
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        self.0.push(*snapshot);
        true
    }
}

fn employment(class: TaxClass, children_tenths: u16) -> Employment {
    let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
    Employment::new(
        insured,
        class,
        children_tenths,
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        },
        None,
    )
    .unwrap()
}

fn household(gross: i64, class: TaxClass, children_tenths: u16) -> Household {
    Household::starting_fresh(
        TaxYear::new(2026).unwrap(),
        1,
        employment(class, children_tenths),
        Money::from_euro(gross).unwrap(),
        Money::ZERO,
    )
    .unwrap()
}

fn run(h: &Household, years: u32) -> Vec<MonthSnapshot> {
    let config = SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal);
    let mut sink = Collect(Vec::new());
    simulate(h, &config, &mut sink).expect("simulates");
    sink.0
}

/// Every settlement on a timeline, as (month index, cents).
fn settlements(months: &[MonthSnapshot]) -> Vec<(u32, i64)> {
    months
        .iter()
        .filter(|m| !m.tax_settlement.is_zero())
        .map(|m| (m.month_index, m.tax_settlement.cents()))
        .collect()
}

// -------------------------------------------------------------------------
// The cross-check
// -------------------------------------------------------------------------

/// Withholding is built to be exactly right for a flat year, so the assessment must return
/// almost nothing. See the module documentation for why this is the file's central test.
///
/// The observed gaps are 96 to 332 **cents** against withholding of 2 248 € to 59 917 € — the
/// two statutes agreeing to within a rounding error over a sixfold salary range, computed by
/// code that shares nothing but the tariff.
#[test]
fn withholding_and_the_annual_assessment_agree_on_a_flat_year() {
    for gross in [2_500_i64, 4_000, 6_000, 9_000, 15_000] {
        let months = run(&household(gross, TaxClass::Class1, 0), 2);
        let first_year_withheld: i64 = months
            .iter()
            .take(12)
            .map(|m| m.income_tax.cents() + m.solidarity_surcharge.cents())
            .sum();

        let found = settlements(&months);
        assert_eq!(found.len(), 1, "one year should settle inside two years");
        let (_, amount) = found[0];

        assert!(
            amount.abs() < 500,
            "at {gross} EUR a month the two paths differ by {amount} cents, which is too \
             much for a flat year — withholding and the assessment should nearly agree"
        );
        // And the agreement is tight relative to the tax itself: well under a tenth of a
        // percent, which an absolute bound alone would not establish at the top of the range.
        assert!(
            amount.abs() * 1_000 < first_year_withheld,
            "a {amount} cent gap on {first_year_withheld} cents withheld is too wide"
        );
    }
}

/// The same agreement must hold under the Splittingtarif, which is a different code path in
/// both the withholding and the assessment.
#[test]
fn the_two_paths_also_agree_for_a_single_earner_couple() {
    for gross in [4_000_i64, 9_000, 15_000] {
        let months = run(&household(gross, TaxClass::Class3, 0), 2);
        let found = settlements(&months);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].1.abs() < 500,
            "class III at {gross} EUR settled {} cents",
            found[0].1
        );
    }
}

// -------------------------------------------------------------------------
// Where withholding and the assessment genuinely diverge
// -------------------------------------------------------------------------

/// The § 31 Günstigerprüfung, appearing in the projection on its own.
///
/// A child changes nothing about the Lohnsteuer — the Kinderfreibetrag affects only the
/// surcharges during the year — so the whole of the relief arrives as a refund, and only
/// where the allowance beats the Kindergeld. That crossover is a real feature of the statute
/// and the kernel reproduces it without being told about it: nothing at 4 000 € a month,
/// hundreds of euro at 9 000 €.
#[test]
fn a_child_produces_a_refund_only_once_the_allowance_beats_the_kindergeld() {
    let refund_with_one_child = |gross: i64| {
        let with = settlements(&run(&household(gross, TaxClass::Class1, 10), 2))[0].1;
        let without = settlements(&run(&household(gross, TaxClass::Class1, 0), 2))[0].1;
        with - without
    };

    // Low income: the Kindergeld wins, so the allowance is not granted and nothing changes.
    assert_eq!(
        refund_with_one_child(2_500),
        0,
        "at 2 500 EUR the Kindergeld should still be the better deal"
    );
    assert_eq!(refund_with_one_child(4_000), 0);

    // Higher income: the allowance wins, and its value arrives as a refund.
    assert!(
        refund_with_one_child(9_000) > 50_000,
        "at 9 000 EUR one child should be worth well over 500 EUR at assessment"
    );
    // And the relief grows with income, since it is worth the marginal rate.
    assert!(refund_with_one_child(9_000) >= refund_with_one_child(6_000));
}

/// The case the feature exists for: a year with unpaid leave in it.
///
/// § 39b taxes each month as though the year continued unchanged, so the months actually
/// worked are withheld at the rate of a full year's salary. The assessment then applies the
/// tariff to what was really earned, and the difference comes back. Before this test the
/// projection showed a career break costing strictly more than it does.
#[test]
fn an_interrupted_year_produces_a_refund_that_withholding_alone_would_miss() {
    let mut broken = household(5_000, TaxClass::Class1, 0);
    broken.schedule = Schedule::new()
        .with(Event::UnpaidLeave {
            from_month: 3,
            until_month: Some(8),
        })
        .unwrap();

    let interrupted = settlements(&run(&broken, 2))[0].1;
    let flat = settlements(&run(&household(5_000, TaxClass::Class1, 0), 2))[0].1;

    assert!(
        interrupted > flat,
        "an interrupted year should refund more than a flat one: {interrupted} vs {flat}"
    );
    // And by a lot, not a rounding difference — six months of over-withholding.
    assert!(
        interrupted > 100_000,
        "six unpaid months should refund well over 1 000 EUR, not {interrupted} cents"
    );
}

/// Starting work in July is the same phenomenon in its purest form: six months of salary
/// withheld at twelve months' rate, and a large refund the following year.
#[test]
fn a_mid_year_start_produces_a_large_refund() {
    let mut july = household(5_000, TaxClass::Class1, 0);
    july.start_month = 7;

    let found = settlements(&run(&july, 3));
    assert!(
        !found.is_empty(),
        "the half year of 2026 should be assessed"
    );
    let (month_index, amount) = found[0];

    // The first calendar year holds six months, ending at index 5, so it settles at 5 + 7.
    assert_eq!(month_index, 5 + SETTLEMENT_LAG_MONTHS);
    assert!(
        amount > 200_000,
        "half a year at 5 000 EUR should refund over 2 000 EUR, not {amount} cents"
    );
}

// -------------------------------------------------------------------------
// When the money arrives
// -------------------------------------------------------------------------

/// A refund is not current-year cash. It must land in the following year, at the lag, or a
/// household planning around a career break would see the money too early.
#[test]
fn the_settlement_arrives_at_the_stated_lag_in_the_following_year() {
    let months = run(&household(5_000, TaxClass::Class1, 10), 4);
    let found = settlements(&months);
    assert!(found.len() >= 2, "several years should settle");

    for (index, (month_index, _)) in found.iter().enumerate() {
        // Year `index` ends at month 12·(index+1) − 1, and settles `lag` months later.
        let year_end = 12 * (u32::try_from(index).unwrap() + 1) - 1;
        assert_eq!(*month_index, year_end + SETTLEMENT_LAG_MONTHS);
        // Which is inside the following calendar year, never the year assessed.
        let snapshot = &months[usize::try_from(*month_index).unwrap()];
        assert_eq!(snapshot.year, 2026 + u16::try_from(index).unwrap() + 1);
        assert_eq!(snapshot.month, u8::try_from(SETTLEMENT_LAG_MONTHS).unwrap());
    }
}

/// The final calendar year is deliberately not assessed: its settlement would fall past the
/// horizon, and a refund arriving after the projection ends is one the household does not
/// have yet. A three-year run therefore settles twice, not three times.
#[test]
fn the_final_year_is_not_settled_inside_the_horizon() {
    assert_eq!(
        settlements(&run(&household(5_000, TaxClass::Class1, 10), 3)).len(),
        2
    );
    assert_eq!(
        settlements(&run(&household(5_000, TaxClass::Class1, 10), 5)).len(),
        4
    );
}

// -------------------------------------------------------------------------
// Where the kernel declines to assess
// -------------------------------------------------------------------------

/// A household with a working spouse must get no settlement at all. Assessing one salary of
/// two under the Splittingtarif would invent a large refund every single year, which is far
/// worse than showing withholding and saying why.
#[test]
fn a_household_with_a_working_spouse_is_not_assessed() {
    for class in [TaxClass::Class4, TaxClass::Class5] {
        let months = run(&household(5_000, class, 0), 4);
        assert!(
            settlements(&months).is_empty(),
            "class {class:?} should not be assessed at all"
        );
        assert_eq!(
            filing_status_for(&employment(class, 0)),
            Err(NoAssessment::SpouseIncomeUnknown)
        );
    }
}

/// And declining must be inert: the projection is exactly what it was before assessments
/// existed, rather than subtly different.
#[test]
fn declining_to_assess_leaves_the_projection_untouched() {
    let months = run(&household(5_000, TaxClass::Class4, 0), 6);
    let mut previous = Money::ZERO;
    for m in &months {
        assert_eq!(m.tax_settlement, Money::ZERO);
        assert_eq!(
            m.wealth,
            previous
                .add(m.savings)
                .unwrap()
                .add(m.investment_return)
                .unwrap()
        );
        previous = m.wealth;
    }
}

// -------------------------------------------------------------------------
// The summary
// -------------------------------------------------------------------------

/// The summary must agree with the timeline it summarises, settlements included — and the
/// household's real tax burden is what was withheld less what came back.
#[test]
fn the_summary_accounts_for_every_settlement() {
    let h = household(9_000, TaxClass::Class1, 10);
    let config = SimulationConfig::conservative(Horizon::years(6).unwrap(), Basis::Nominal);

    let mut summary = Summary::default();
    simulate(&h, &config, &mut summary).expect("simulates");
    let months = run(&h, 6);
    let found = settlements(&months);

    assert_eq!(
        summary.settlements_applied,
        u32::try_from(found.len()).unwrap()
    );
    assert_eq!(
        summary.total_settlements.cents(),
        found.iter().map(|(_, amount)| *amount).sum::<i64>()
    );
    // With a child above the crossover the settlements are refunds, so the real burden is
    // strictly below what was withheld.
    assert!(summary.total_settlements > Money::ZERO);
    assert!(summary.total_settlements < summary.total_tax);
}

/// A projection that crosses from enacted law into projected years must keep assessing, using
/// each year's own law — and the path it traces is a finding in its own right.
///
/// This household's pay never rises while the Kinderfreibetrag and the Kindergeld are both
/// indexed. Its income therefore falls in real terms, its marginal rate with it, and the
/// § 31 Günstigerprüfung slowly flips: the child allowance is worth 372.96 € at the start,
/// declines every year, and by about year eleven the Kindergeld has become the better deal.
/// After that the settlement is just the ordinary flat-year residual of a euro or two.
///
/// Nobody encoded that crossover. It falls out of two indexed statutory series meeting a
/// household that stood still, which is exactly the kind of thing a projection is for.
///
/// This test also guards a bug it found: the settlement used to keep falling *past* zero
/// into a 169 € demand, because `project_payroll` indexed the Pauschbeträge while
/// `project_deductions` carried them forward, and the two paths drifted about ten euro a
/// year. See `the_two_paths_project_the_same_pauschbetraege` in `casivell-projection`.
#[test]
fn the_child_allowance_loses_to_the_kindergeld_as_flat_pay_erodes() {
    let found = settlements(&run(&household(6_000, TaxClass::Class1, 10), 20));
    assert_eq!(found.len(), 19, "every year but the last should settle");

    // It starts as a substantial refund and declines monotonically while the allowance is
    // still winning. Once it has flipped, the remaining figure is the flat-year residual and
    // moves by a few cents either way with the rounding, which is not a trend.
    assert!(
        found[0].1 > 30_000,
        "the allowance should start out worth over 300 EUR"
    );
    let still_material = |cents: i64| cents > 1_000;
    for pair in found.windows(2) {
        if still_material(pair[0].1) {
            assert!(
                pair[1].1 <= pair[0].1,
                "the relief should never grow for a household standing still: {} then {}",
                pair[0].1,
                pair[1].1
            );
        }
    }
    assert!(
        found.iter().any(|(_, cents)| !still_material(*cents)),
        "the allowance should stop winning within twenty years"
    );

    // And it settles onto the flat-year residual rather than running away downward. A
    // divergence would show up here as an ever-growing demand.
    let (_, last) = found[found.len() - 1];
    assert!(
        (0..500).contains(&last),
        "the tail should be the ordinary flat-year residual, not {last} cents"
    );
}
