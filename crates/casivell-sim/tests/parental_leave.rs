//! Parental leave in the kernel: the benefit, and the tax it causes.
//!
//! # Why this is one file and not two
//!
//! Elterngeld and the Progressionsvorbehalt are the same event seen a year apart. The benefit
//! arrives untaxed and monthly; the tax it causes arrives once, in a lump, the following
//! summer, when the money is long spent. Testing them separately would let each look correct
//! while the pair told a household the wrong thing.
//!
//! `the_progressionsvorbehalt_claws_back_part_of_the_benefit` is the file's point: it isolates
//! the § 32b cost by running the *same* interruption twice, once with Elterngeld and once
//! without, and finds the household 2 523 € worse off in tax for having taken the benefit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_benefits::Variant;
use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::{Employment, HealthCover};
use casivell_sim::{
    Basis, Event, Horizon, Household, MonthSnapshot, Schedule, SimulationConfig, Sink, Summary,
    simulate,
};
use casivell_social::Insured;

struct Collect(Vec<MonthSnapshot>);

impl Sink for Collect {
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        self.0.push(*snapshot);
        true
    }
}

fn euro(amount: i64) -> Money {
    Money::from_euro(amount).unwrap()
}

fn household(gross: i64, event: Option<Event>) -> Household {
    let insured = Insured::new(30, true, 1, Bundesland::NordrheinWestfalen, None).unwrap();
    let employment = Employment::new(
        insured,
        TaxClass::Class1,
        10,
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        },
        None,
    )
    .unwrap();
    let mut household = Household::starting_fresh(
        TaxYear::new(2026).unwrap(),
        1,
        employment,
        euro(gross),
        euro(1_500),
    )
    .unwrap();
    if let Some(event) = event {
        household.schedule = Schedule::new().with(event).unwrap();
    }
    household
}

fn config(years: u32) -> SimulationConfig {
    SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal)
}

fn months(household: &Household, years: u32) -> Vec<MonthSnapshot> {
    let mut sink = Collect(Vec::new());
    simulate(household, &config(years), &mut sink).expect("simulates");
    sink.0
}

fn summary(household: &Household, years: u32) -> Summary {
    let mut summary = Summary::default();
    simulate(household, &config(years), &mut summary).expect("simulates");
    summary
}

/// Twelve months of Basiselterngeld beginning in July of the first year, so that both
/// affected calendar years hold salary *and* benefit — which is what makes § 32b bite.
fn leave(from_month: u32, months: u32) -> Event {
    Event::ParentalLeave {
        from_month,
        months,
        working_fraction: Rate::ZERO,
        variant: Variant::Basis,
        sibling_bonus: false,
        additional_children: 0,
    }
}

// -------------------------------------------------------------------------
// The benefit
// -------------------------------------------------------------------------

/// Elterngeld must be paid for exactly the months requested, at a constant amount, and
/// employment income must stop for the same months.
#[test]
fn parental_leave_pays_elterngeld_for_exactly_the_months_requested() {
    let timeline = months(&household(4_000, Some(leave(6, 12))), 4);

    let paid: Vec<_> = timeline
        .iter()
        .filter(|m| !m.parental_benefit.is_zero())
        .collect();
    assert_eq!(paid.len(), 12);
    assert_eq!(paid[0].month_index, 6);
    assert_eq!(paid[11].month_index, 17);

    let amount = paid[0].parental_benefit;
    assert!(amount > euro(1_500) && amount < euro(1_800));
    for m in &paid {
        assert_eq!(m.parental_benefit, amount, "the amount must not drift");
        assert_eq!(m.gross, Money::ZERO, "a full break earns no salary");
        assert!(m.employment_interrupted);
    }

    // And nothing outside the window.
    assert_eq!(timeline[5].parental_benefit, Money::ZERO);
    assert_eq!(timeline[18].parental_benefit, Money::ZERO);
    assert!(timeline[18].gross > Money::ZERO, "work resumes");
}

/// The amount is fixed by the twelve months before the birth and does not move afterwards.
///
/// A household whose pay is growing at 5 % a year must not see its Elterngeld grow with it:
/// the BEEG measures the Bemessungszeitraum once. Recomputing monthly from the current
/// baseline would let the benefit drift upward, which is the same class of bug that
/// `Event::PayChange` had before rebasing and modifying were separated.
#[test]
fn the_benefit_is_fixed_at_the_start_of_the_leave() {
    let mut h = household(4_000, Some(leave(6, 14)));
    h.annual_pay_growth = Rate::from_percent_millis(5_000).unwrap();

    let timeline = months(&h, 4);
    let paid: Vec<_> = timeline
        .iter()
        .filter(|m| !m.parental_benefit.is_zero())
        .map(|m| m.parental_benefit)
        .collect();

    assert_eq!(paid.len(), 14);
    // The leave runs months 6..=19 and crosses two employment anniversaries, at months 12
    // and 24, where pay steps up 5 %. Month 21 is the first back at work.
    assert_eq!(timeline[19].gross, Money::ZERO, "still on leave");
    assert!(timeline[21].gross > timeline[5].gross, "pay grew meanwhile");
    // The benefit does not follow it.
    assert!(
        paid.iter().all(|amount| *amount == paid[0]),
        "the benefit drifted with the salary: {paid:?}"
    );
}

// -------------------------------------------------------------------------
// The Progressionsvorbehalt — the point of the file
// -------------------------------------------------------------------------

/// Taking Elterngeld costs this household 2 523 € in extra tax, and it is invisible until the
/// following summer.
///
/// Isolated by running the identical interruption twice — once as `ParentalLeave`, once as
/// `UnpaidLeave` — so the employment income, the withholding and everything else are the same
/// and only the benefit differs. Withholding is byte-for-byte identical in both runs, because
/// § 32b changes nothing during the year. What changes is the settlement: the refund falls
/// from 4 216,48 € to 1 693,48 €.
///
/// That is 12,8 % of a 19 700 € benefit taken back through the rate on the household's other
/// income — a real cost that no payslip shows, that arrives a year late, and that a projection
/// omitting § 32b would have told the household it did not have to pay.
#[test]
fn the_progressionsvorbehalt_claws_back_part_of_the_benefit() {
    let with_benefit = summary(&household(4_000, Some(leave(6, 12))), 4);
    let without = summary(
        &household(
            4_000,
            Some(Event::UnpaidLeave {
                from_month: 6,
                until_month: Some(18),
            }),
        ),
        4,
    );

    // Identical employment income, so identical withholding.
    assert_eq!(
        with_benefit.total_tax, without.total_tax,
        "§ 32b must change nothing during the year"
    );

    // But a materially smaller refund.
    let clawback = without
        .total_settlements
        .sub(with_benefit.total_settlements)
        .unwrap();
    assert!(
        clawback > euro(2_000) && clawback < euro(3_000),
        "the Progressionsvorbehalt clawed back {clawback:?}, outside the expected band"
    );

    // And it is a real but minority share of the benefit — the household is still far better
    // off having taken it, which is the other half of the honest answer.
    assert!(with_benefit.total_parental_benefit > euro(19_000));
    assert!(clawback.cents() * 3 < with_benefit.total_parental_benefit.cents());

    // The wealth difference reconciles exactly: benefit received less tax clawed back.
    let wealth_gain = with_benefit.final_wealth.sub(without.final_wealth).unwrap();
    assert_eq!(
        wealth_gain,
        with_benefit.total_parental_benefit.sub(clawback).unwrap()
    );
}

/// A household on leave for a whole calendar year pays *nothing* to the Progressionsvorbehalt,
/// because the rate applies to a taxable income of zero.
///
/// § 32b raises a rate; it does not tax the benefit. With no other income there is nothing for
/// the raised rate to apply to. This is worth a test because it is the case people expect to
/// be worst and it is in fact free — and because a model that taxed the benefit directly would
/// fail here loudly.
#[test]
fn a_whole_calendar_year_of_leave_costs_nothing_in_progression() {
    // Leave running exactly from January to December of the second year.
    let with_benefit = summary(&household(4_000, Some(leave(12, 12))), 4);
    let without = summary(
        &household(
            4_000,
            Some(Event::UnpaidLeave {
                from_month: 12,
                until_month: Some(24),
            }),
        ),
        4,
    );

    assert_eq!(
        with_benefit.total_settlements, without.total_settlements,
        "with no other income in the year, § 32b can cost nothing"
    );
    assert!(with_benefit.total_parental_benefit > euro(19_000));
}

// -------------------------------------------------------------------------
// ElterngeldPlus and part-time leave
// -------------------------------------------------------------------------

/// `ElterngeldPlus` over a doubled window pays about the same in total as Basiselterngeld does
/// over the short one, which is the trade the provision offers.
#[test]
fn elterngeld_plus_spreads_a_similar_total_over_twice_the_months() {
    let basis = summary(&household(4_000, Some(leave(6, 12))), 5);
    let plus = summary(
        &household(
            4_000,
            Some(Event::ParentalLeave {
                from_month: 6,
                months: 24,
                working_fraction: Rate::ZERO,
                variant: Variant::Plus,
                sibling_bonus: false,
                additional_children: 0,
            }),
        ),
        5,
    );

    let difference = plus
        .total_parental_benefit
        .sub(basis.total_parental_benefit)
        .unwrap();
    assert!(
        difference.cents().abs() < euro(50).cents(),
        "twice the months at half the rate should total the same: {:?} against {:?}",
        plus.total_parental_benefit,
        basis.total_parental_benefit
    );
}

/// Working part time during the leave earns salary *and* benefit, and the § 2 Abs. 3
/// difference rule reduces the benefit accordingly.
#[test]
fn part_time_leave_earns_both_salary_and_a_reduced_benefit() {
    let part_time = Event::ParentalLeave {
        from_month: 6,
        months: 12,
        working_fraction: Rate::from_percent_millis(50_000).unwrap(),
        variant: Variant::Basis,
        sibling_bonus: false,
        additional_children: 0,
    };
    let timeline = months(&household(4_000, Some(part_time)), 3);
    let during = &timeline[8];

    assert!(during.gross > Money::ZERO, "part time still earns");
    assert_eq!(during.gross, euro(2_000));
    assert!(!during.employment_interrupted);
    assert!(during.working_time_reduced);

    // The benefit is positive but smaller than a full break's would be.
    let full_break = months(&household(4_000, Some(leave(6, 12))), 3)[8].parental_benefit;
    assert!(during.parental_benefit > Money::ZERO);
    assert!(during.parental_benefit < full_break);
}

/// The case that surprises people: parental leave ending in a **demand**.
///
/// Every other scenario here produces a refund, because stopping work mid-year over-withholds
/// and the assessment gives it back. Part-time `ElterngeldPlus` does not: the household earns
/// a full year of (reduced) salary *and* draws Elterngeld alongside it, so withholding is
/// roughly right for the salary and § 32b then raises the rate on all of it. There is no
/// over-withholding left to absorb the cost.
///
/// The result is a bill of about 1 200 € a year, arriving the summer after each year of leave,
/// for a household whose income went *down*. This is the single most useful thing the
/// Progressionsvorbehalt modelling produces, and no payslip anywhere in that period hints at
/// it.
#[test]
fn part_time_leave_across_full_years_produces_a_demand_not_a_refund() {
    let plus = Event::ParentalLeave {
        from_month: 24,
        months: 28,
        working_fraction: Rate::from_percent_millis(50_000).unwrap(),
        variant: Variant::Plus,
        sibling_bonus: false,
        additional_children: 0,
    };
    let timeline = months(&household(4_000, Some(plus)), 6);

    let demands: Vec<_> = timeline
        .iter()
        .filter(|m| m.tax_settlement.is_negative())
        .collect();
    assert!(
        demands.len() >= 2,
        "the years fully inside the leave should each end in a demand"
    );
    // Every demand arrives the following July.
    for demand in &demands {
        assert_eq!(demand.month, 7);
    }
    // The leave spans two whole calendar years and part of a third, so two of the demands are
    // full-year ones of about 1 200 € and the last is a smaller partial-year figure.
    let substantial = demands
        .iter()
        .filter(|m| m.tax_settlement.neg().unwrap() > euro(500))
        .count();
    assert!(
        substantial >= 2,
        "the two full years of leave should each cost about 1 200 EUR: {:?}",
        demands.iter().map(|m| m.tax_settlement).collect::<Vec<_>>()
    );

    // The same reduced hours *without* Elterngeld also end in a demand — § 39b under-withholds
    // whenever income rises mid-year, so returning to full time does it on its own. The
    // control is therefore quantitative rather than a sign test: § 32b makes the demand very
    // much larger.
    let bare = summary(
        &household(
            4_000,
            Some(Event::WorkingTime {
                from_month: 24,
                until_month: Some(52),
                fraction: Rate::from_percent_millis(50_000).unwrap(),
            }),
        ),
        6,
    );
    let with_benefit = summary(&household(4_000, Some(plus)), 6);

    assert!(
        with_benefit.total_settlements < bare.total_settlements,
        "the benefit must make the settlement worse, not better"
    );
    let cost = bare
        .total_settlements
        .sub(with_benefit.total_settlements)
        .unwrap();
    assert!(
        cost > euro(2_000),
        "§ 32b on nearly three years of ElterngeldPlus should cost well over 2 000 EUR, not {cost:?}"
    );
}

// -------------------------------------------------------------------------
// Bookkeeping
// -------------------------------------------------------------------------

/// Wealth must still move by exactly its stated parts, benefit included.
#[test]
fn wealth_accounts_for_the_benefit() {
    let timeline = months(&household(4_000, Some(leave(6, 12))), 4);
    let mut previous = Money::ZERO;
    for m in &timeline {
        let expected = previous
            .add(m.investment_return)
            .unwrap()
            .add(m.savings)
            .unwrap()
            .add(m.tax_settlement)
            .unwrap();
        assert_eq!(
            m.wealth, expected,
            "wealth drifted at month {}",
            m.month_index
        );
        previous = m.wealth;
    }
    // And the benefit reached the household through savings rather than being lost.
    let during = &timeline[8];
    assert_eq!(
        during.savings,
        during
            .net
            .add(during.parental_benefit)
            .unwrap()
            .sub(during.expenses)
            .unwrap()
    );
}

/// The summary must agree with the timeline about how much benefit was paid.
#[test]
fn the_summary_totals_the_benefit() {
    let h = household(4_000, Some(leave(6, 14)));
    let timeline = months(&h, 4);
    let total: i64 = timeline.iter().map(|m| m.parental_benefit.cents()).sum();
    assert_eq!(summary(&h, 4).total_parental_benefit.cents(), total);
    assert!(total > 0);
}

/// A household with no parental leave must be entirely unaffected.
#[test]
fn a_household_without_leave_is_untouched() {
    let plain = summary(&household(4_000, None), 5);
    assert_eq!(plain.total_parental_benefit, Money::ZERO);
    for m in months(&household(4_000, None), 5) {
        assert_eq!(m.parental_benefit, Money::ZERO);
    }
}
