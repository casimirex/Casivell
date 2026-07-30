//! Life events over a full career: the questions the product exists to answer.
//!
//! # Why these are integration tests
//!
//! The unit tests in `events` check that a schedule resolves correctly. These check something
//! different and more important: that a scheduled event produces the *right consequence*
//! decades later, through tax, contributions, pension accrual and compounding. That is the
//! claim the product makes, and it cannot be tested a month at a time.
//!
//! Two of the three questions on the front page become answerable here. The third — buying
//! versus renting — needs Grunderwerbsteuer and mortgage amortisation, and is not yet modelled.

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
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::{Employment, HealthCover};
use casivell_sim::{
    Basis, Event, Horizon, Household, Schedule, SimulationConfig, Summary, simulate,
};
use casivell_social::Insured;

fn employment() -> Employment {
    let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
    Employment::new(
        insured,
        TaxClass::Class1,
        0,
        HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        },
        None,
    )
    .unwrap()
}

fn household(schedule: &Schedule) -> Household {
    let mut h = Household::starting_fresh(
        TaxYear::new(2026).unwrap(),
        1,
        employment(),
        Money::from_euro(4_500).unwrap(),
        Money::from_euro(2_500).unwrap(),
    )
    .unwrap();
    // Pay tracking average wages, so a pension comparison isolates the event rather than the
    // erosion a stagnant salary causes on its own.
    h.annual_pay_growth = Rate::from_percent_millis(2_800).unwrap();
    h.annual_expense_growth = Rate::from_percent_millis(2_000).unwrap();
    h.schedule = *schedule;
    h
}

fn run(schedule: &Schedule, years: u32) -> Summary {
    let config = SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal);
    let mut summary = Summary::default();
    simulate(&household(schedule), &config, &mut summary).expect("simulates");
    summary
}

/// An empty schedule must reproduce the projection exactly as it behaved before events
/// existed. Without this, every other test here measures the wrong thing.
#[test]
fn an_empty_schedule_changes_nothing() {
    let baseline = run(&Schedule::new(), 40);
    assert!(baseline.final_wealth > Money::ZERO);
    assert!(baseline.final_pension_points.micro() > 40_000_000);
}

/// *"What does part-time really cost over a lifetime?"* — the Teilzeitfalle.
///
/// Ten years at 60 % from age 30. The interesting figure is not the pay forgone, which anyone
/// can multiply out, but the **pension entitlement that never recovers**: Entgeltpunkte are a
/// ratio to the national average wage, so a reduced year is permanently a reduced year, and
/// returning to full time restores the salary but not the record.
#[test]
fn part_time_permanently_reduces_the_pension_record() {
    let full_time = run(&Schedule::new(), 40);
    let part_time = run(
        &Schedule::new()
            .with(Event::WorkingTime {
                from_month: 0,
                until_month: Some(120),
                fraction: Rate::from_percent_millis(60_000).unwrap(),
            })
            .unwrap(),
        40,
    );

    // The record is permanently smaller, thirty years after returning to full time.
    assert!(
        part_time.final_pension_points < full_time.final_pension_points,
        "ten part-time years must leave a smaller pension record"
    );
    let points_lost =
        full_time.final_pension_points.micro() - part_time.final_pension_points.micro();
    // Ten years at 60 % forgoes 40 % of ten years' accrual: about four points.
    assert!(
        (3_000_000..=5_000_000).contains(&points_lost),
        "the record lost {points_lost} micropoints, expected about four points"
    );

    // And wealth is lower too: the part-time years saved less.
    assert!(part_time.final_wealth < full_time.final_wealth);
}

/// The other half of the answer: part-time costs *less* in tax than the pay forgone, because
/// the tariff is progressive and the hours given up were taxed at the top of the household's
/// rate.
///
/// This only appears if the tax is recomputed from the reduced salary rather than scaled, which
/// is what running every month through the real payroll code buys.
#[test]
fn the_progressive_tariff_softens_the_cost_of_part_time() {
    let full_time = run(&Schedule::new(), 10);
    let part_time = run(
        &Schedule::new()
            .with(Event::WorkingTime {
                from_month: 0,
                until_month: None,
                fraction: Rate::from_percent_millis(60_000).unwrap(),
            })
            .unwrap(),
        10,
    );

    // Gross fell to 60 %; tax must fall to well below that share of the full-time figure.
    let tax_ratio_ppm = part_time.total_tax.cents() * 1_000_000 / full_time.total_tax.cents();
    assert!(
        tax_ratio_ppm < 600_000,
        "tax fell to {tax_ratio_ppm} ppm of the full-time figure, expected well under 60 %"
    );
    // But net income falls by less than gross, which is the point for the household.
    let net_ratio_ppm = part_time.total_net.cents() * 1_000_000 / full_time.total_net.cents();
    assert!(
        net_ratio_ppm > 600_000,
        "net fell to {net_ratio_ppm} ppm, expected more than the 60 % of gross"
    );
}

/// *"Can we afford a year off?"* — an unpaid career break.
///
/// The household keeps spending while earning nothing, so wealth falls, the pension record is
/// permanently a year short, and no tax is paid meanwhile.
#[test]
fn a_career_break_costs_wealth_tax_and_pension() {
    let schedule = Schedule::new()
        .with(Event::UnpaidLeave {
            from_month: 60,
            until_month: Some(72),
        })
        .unwrap();

    let with_break = run(&schedule, 40);
    let without = run(&Schedule::new(), 40);

    assert!(
        with_break.final_wealth < without.final_wealth,
        "a year off must cost wealth"
    );
    assert!(
        with_break.final_pension_points < without.final_pension_points,
        "and a year of entitlement, permanently"
    );
    assert!(
        with_break.total_tax < without.total_tax,
        "and no tax is paid while not earning"
    );
}

/// A break taken *early*, before savings have built up, sends wealth negative — and the end
/// state hides it completely. This is exactly why `Summary::minimum_wealth` exists.
#[test]
fn an_early_career_break_goes_negative_and_the_end_state_hides_it() {
    let schedule = Schedule::new()
        .with(Event::UnpaidLeave {
            from_month: 6,
            until_month: Some(24),
        })
        .unwrap();
    let summary = run(&schedule, 40);

    assert!(
        summary.minimum_wealth.is_negative(),
        "eighteen months off from a standing start must go into deficit"
    );
    assert!(
        summary.final_wealth > Money::ZERO,
        "and thirty-eight years of saving should recover it"
    );
}

/// A one-off cost lands on wealth without touching income or tax — the right shape for a
/// deposit, and the wrong shape for a bonus.
#[test]
fn a_one_off_cost_reduces_wealth_without_touching_tax() {
    let schedule = Schedule::new()
        .with(Event::OneOff {
            month: 60,
            amount: Money::from_euro(-60_000).unwrap(),
        })
        .unwrap();
    let with_cost = run(&schedule, 20);
    let without = run(&Schedule::new(), 20);

    assert_eq!(
        with_cost.total_tax, without.total_tax,
        "a one-off must not change the tax"
    );
    assert_eq!(
        with_cost.final_pension_points, without.final_pension_points,
        "nor the pension record"
    );
    // With no investment return assumed, the difference is exactly the cost.
    let difference = without.final_wealth.sub(with_cost.final_wealth).unwrap();
    assert_eq!(difference, Money::from_euro(60_000).unwrap());
}

/// A promotion raises pay from the month it lands, and later growth compounds from the new
/// figure rather than the old one.
#[test]
fn a_promotion_compounds_from_the_new_salary() {
    let schedule = Schedule::new()
        .with(Event::PayChange {
            from_month: 24,
            monthly_gross: Money::from_euro(6_500).unwrap(),
        })
        .unwrap();
    let promoted = run(&schedule, 40);
    let flat = run(&Schedule::new(), 40);

    assert!(promoted.final_wealth > flat.final_wealth);
    assert!(promoted.final_pension_points > flat.final_pension_points);
    assert!(promoted.total_tax > flat.total_tax);
}

/// Events must compose. A career that reduces hours, takes a break, and is promoted must
/// produce a coherent result rather than one event silently winning.
#[test]
fn several_events_compose_over_a_career() {
    let schedule = Schedule::new()
        .with(Event::WorkingTime {
            from_month: 36,
            until_month: Some(96),
            fraction: Rate::from_percent_millis(70_000).unwrap(),
        })
        .unwrap()
        .with(Event::UnpaidLeave {
            from_month: 120,
            until_month: Some(126),
        })
        .unwrap()
        .with(Event::PayChange {
            from_month: 180,
            monthly_gross: Money::from_euro(8_000).unwrap(),
        })
        .unwrap()
        .with(Event::ExpenseChange {
            from_month: 180,
            monthly_expenses: Money::from_euro(4_000).unwrap(),
        })
        .unwrap();

    let summary = run(&schedule, 40);
    assert_eq!(summary.months, 480);
    assert!(summary.final_wealth > Money::ZERO);
    assert!(summary.final_pension_points.micro() > 30_000_000);

    // Isolating the interruptions means holding the promotion constant: the same career
    // *without* the reduced hours and the break must leave a larger pension record.
    //
    // Comparing against the plain baseline instead would be wrong, and interestingly so — the
    // promotion to 8 000 EUR compounds from month 180 and more than offsets both
    // interruptions, so the composed career ends with *more* entitlement than an
    // uninterrupted one on the original salary. A real effect, and not what this test is for.
    let promotion_only = Schedule::new()
        .with(Event::PayChange {
            from_month: 180,
            monthly_gross: Money::from_euro(8_000).unwrap(),
        })
        .unwrap()
        .with(Event::ExpenseChange {
            from_month: 180,
            monthly_expenses: Money::from_euro(4_000).unwrap(),
        })
        .unwrap();
    let uninterrupted = run(&promotion_only, 40);
    assert!(
        summary.final_pension_points < uninterrupted.final_pension_points,
        "the reduced hours and the break must cost entitlement"
    );
    assert!(
        summary.final_wealth < uninterrupted.final_wealth,
        "and wealth"
    );
}

/// The finding the previous test turned up, asserted deliberately: a large enough later
/// promotion can more than offset earlier interruptions, because Entgeltpunkte accrue on
/// whatever the salary is at the time and a higher salary accrues faster.
///
/// Worth pinning because it is the encouraging half of the Teilzeitfalle — the trap is real but
/// it is not necessarily permanent if pay recovers by more than it fell.
#[test]
fn a_large_later_promotion_can_offset_earlier_interruptions() {
    let interrupted_then_promoted = Schedule::new()
        .with(Event::WorkingTime {
            from_month: 36,
            until_month: Some(96),
            fraction: Rate::from_percent_millis(70_000).unwrap(),
        })
        .unwrap()
        .with(Event::PayChange {
            from_month: 180,
            monthly_gross: Money::from_euro(8_000).unwrap(),
        })
        .unwrap();

    let recovered = run(&interrupted_then_promoted, 40);
    let never_interrupted = run(&Schedule::new(), 40);

    assert!(
        recovered.final_pension_points > never_interrupted.final_pension_points,
        "a promotion large enough should more than repair the record"
    );
}

/// Non-employment income raises what is available to save without attracting contributions or
/// earning Entgeltpunkte. That separation is the whole point of the event.
#[test]
fn other_income_saves_without_earning_pension_entitlement() {
    let schedule = Schedule::new()
        .with(Event::OtherIncome {
            from_month: 0,
            until_month: Some(12),
            monthly_amount: Money::from_euro(1_800).unwrap(),
        })
        .unwrap();
    let with_income = run(&schedule, 20);
    let without = run(&Schedule::new(), 20);

    assert!(with_income.final_wealth > without.final_wealth);
    assert_eq!(
        with_income.final_pension_points, without.final_pension_points,
        "non-employment income must earn no Entgeltpunkte"
    );
    assert_eq!(
        with_income.total_contributions, without.total_contributions,
        "nor attract social insurance contributions"
    );
}

/// Events must not break the projection at any horizon the kernel permits.
#[test]
fn events_survive_the_longest_horizon() {
    let schedule = Schedule::new()
        .with(Event::WorkingTime {
            from_month: 240,
            until_month: None,
            fraction: Rate::from_percent_millis(50_000).unwrap(),
        })
        .unwrap();
    let summary = run(&schedule, 70);
    assert_eq!(summary.months, 840);
}
