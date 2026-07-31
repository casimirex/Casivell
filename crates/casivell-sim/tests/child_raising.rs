//! Kindererziehungszeiten: § 56 and § 70 Abs. 2 SGB VI.
//!
//! # Why this had to be built
//!
//! Casivell exists in part to show what a career break costs a pension. Before this, it showed
//! a break with *no* pension credit at all — which overstated the harm, in the one place the
//! model most needed to be even-handed. § 56 credits thirty-six months of entitlement after a
//! birth, worth about a point a year, and for a modest earner that is most of what the break
//! took away.
//!
//! `the_credit_offsets_most_of_what_a_break_costs_a_pension` is the test that closes it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::{Bundesland, SocialParameters, TaxClass};
use casivell_payroll::{Employment, HealthCover};
use casivell_sim::{
    Basis, Event, Horizon, Household, MonthSnapshot, Schedule, SimulationConfig, Sink, simulate,
};
use casivell_social::{EntgeltPoints, Insured};

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

fn household(gross: i64, events: &[Event]) -> Household {
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
    let mut schedule = Schedule::new();
    for event in events {
        schedule = schedule.with(*event).unwrap();
    }
    household.schedule = schedule;
    household
}

fn months(household: &Household, years: u32) -> Vec<MonthSnapshot> {
    let mut sink = Collect(Vec::new());
    simulate(
        household,
        &SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal),
        &mut sink,
    )
    .expect("simulates");
    sink.0
}

/// Points accrued over the whole run.
fn final_points(household: &Household, years: u32) -> i64 {
    months(household, years)
        .last()
        .unwrap()
        .pension_points
        .micro()
}

fn pension() -> casivell_lawdata::PensionInsurance {
    SocialParameters::for_year(TaxYear::new(2026).unwrap())
        .unwrap()
        .pension
}

// -------------------------------------------------------------------------
// The credit itself
// -------------------------------------------------------------------------

/// Thirty-six months at 0,0833 points is 2,9988 — not three.
///
/// § 70 Abs. 2 states the monthly figure to four decimal places rather than as an exact
/// twelfth, so a year of it is 0,9996 points and three years 2,9988. Rounding that to a round
/// number would be tidier and would not be the statute.
#[test]
fn the_credit_is_the_statutes_truncated_figure_not_a_round_one() {
    let p = pension();
    assert_eq!(p.child_raising_points_micro, 83_300);
    assert_eq!(p.child_raising_months, 36);

    assert_eq!(
        EntgeltPoints::child_raising(12, &p).unwrap().micro(),
        999_600
    );
    assert_eq!(
        EntgeltPoints::child_raising(36, &p).unwrap().micro(),
        2_998_800
    );
    assert!(
        EntgeltPoints::child_raising(36, &p).unwrap().micro() < 3 * EntgeltPoints::MICRO_PER_POINT,
        "three years of credit is just under three points"
    );
}

/// The period begins the month *after* the birth and runs exactly thirty-six months.
#[test]
fn the_period_begins_after_the_month_of_birth() {
    // No salary at all, so every point accrued is child-raising credit and the window is
    // visible directly in the timeline.
    let h = household(
        4_000,
        &[
            Event::ChildBorn { month: 24 },
            Event::UnpaidLeave {
                from_month: 0,
                until_month: None,
            },
        ],
    );
    let timeline = months(&h, 8);

    let gained = |index: usize| {
        timeline[index].pension_points.micro() - timeline[index - 1].pension_points.micro()
    };

    assert_eq!(
        timeline[24].pension_points.micro(),
        0,
        "nothing in the month of birth"
    );
    assert!(gained(25) > 0, "the credit starts the month after");
    assert_eq!(gained(60), 83_300, "the thirty-sixth month still credits");
    assert_eq!(gained(61), 0, "and the thirty-seventh does not");
    assert_eq!(timeline[61].pension_points.micro(), 2_998_800);
}

/// The credit does not depend on taking leave. § 56 credits whoever raises the child, and a
/// parent back at work the following month keeps all thirty-six months of it.
///
/// This is why `ChildBorn` is a separate event from `ParentalLeave`: deriving one from the
/// other would deny the credit to precisely the households that took no leave.
#[test]
fn the_credit_is_earned_without_taking_any_leave() {
    let working = final_points(&household(4_000, &[Event::ChildBorn { month: 12 }]), 8);
    let childless = final_points(&household(4_000, &[]), 8);
    assert_eq!(working - childless, 2_998_800);
}

// -------------------------------------------------------------------------
// § 56 Abs. 5: extension, not overlap
// -------------------------------------------------------------------------

/// Two children born a year apart must yield seventy-two months of credit, not thirty-six
/// doubled up.
///
/// § 56 Abs. 5 Satz 2 extends the period "um die Anzahl an Kalendermonaten der gleichzeitigen
/// Erziehung" rather than running two in parallel. A parent never earns two children's credit
/// for one month — the second child's period simply starts when the first's ends.
#[test]
fn a_second_child_extends_the_period_rather_than_doubling_it() {
    let two = final_points(
        &household(
            4_000,
            &[
                Event::ChildBorn { month: 12 },
                Event::ChildBorn { month: 24 },
            ],
        ),
        12,
    );
    let one = final_points(&household(4_000, &[Event::ChildBorn { month: 12 }]), 12);
    let none = final_points(&household(4_000, &[]), 12);

    assert_eq!(one - none, 2_998_800, "one child: thirty-six months");
    assert_eq!(
        two - none,
        2 * 2_998_800,
        "two children: seventy-two months"
    );
}

/// And no single month ever credits more than one child's worth, however close the births.
#[test]
fn no_month_ever_credits_two_children_at_once() {
    let h = household(
        4_000,
        &[
            Event::ChildBorn { month: 12 },
            Event::ChildBorn { month: 13 },
            Event::ChildBorn { month: 14 },
            // No salary, so the whole gain is child-raising credit.
            Event::UnpaidLeave {
                from_month: 0,
                until_month: None,
            },
        ],
    );
    let timeline = months(&h, 15);
    for pair in timeline.windows(2) {
        let gained = pair[1].pension_points.micro() - pair[0].pension_points.micro();
        assert!(
            gained <= 83_300,
            "a month credited {gained} micropoints, more than one child's worth"
        );
    }
    // Three children still total three full periods, just spread over nine years.
    assert_eq!(
        timeline.last().unwrap().pension_points.micro(),
        3 * 2_998_800
    );
}

/// Well-spaced children get their plain thirty-six months apiece, with the queueing rule
/// reducing to the simple case.
#[test]
fn well_spaced_children_do_not_interact() {
    let spaced = final_points(
        &household(
            4_000,
            &[
                Event::ChildBorn { month: 12 },
                Event::ChildBorn { month: 96 },
            ],
        ),
        20,
    );
    let none = final_points(&household(4_000, &[]), 20);
    assert_eq!(spaced - none, 2 * 2_998_800);
}

// -------------------------------------------------------------------------
// § 70 Abs. 2: the cap
// -------------------------------------------------------------------------

/// The cap protects the people who gave up income, and is worth nothing to those who did not.
///
/// § 70 Abs. 2 Satz 2 caps the year's combined points at what a full-ceiling earner accrues.
/// Someone already earning at the Beitragsbemessungsgrenze is at that cap from their salary
/// alone, so their child-raising credit adds **nothing**; someone on a modest salary receives
/// it in full. That is the provision working as designed rather than a defect, and it is
/// asserted here because it looks like one at first sight.
#[test]
fn the_cap_gives_a_top_earner_nothing_and_a_modest_earner_everything() {
    let gain = |gross: i64| {
        final_points(&household(gross, &[Event::ChildBorn { month: 12 }]), 8)
            - final_points(&household(gross, &[]), 8)
    };

    // Well below the ceiling: the whole credit lands.
    assert_eq!(gain(3_000), 2_998_800);
    // Far above it: the salary alone already reaches the annual maximum.
    assert_eq!(gain(20_000), 0);
    // And somewhere between, a partial credit.
    let partial = gain(7_000);
    assert!(
        partial > 0 && partial < 2_998_800,
        "a middling earner should get part of the credit, not {partial}"
    );
}

/// The annual maximum must be derived from the ceiling, since Anlage 2b stops at 2002.
#[test]
fn the_annual_maximum_is_about_two_points() {
    let maximum = EntgeltPoints::annual_maximum(&pension()).unwrap();
    assert!(
        (1_900_000..=2_100_000).contains(&maximum.micro()),
        "the ceiling is about twice average earnings, so the cap is about 2,0 points, not {}",
        maximum.micro()
    );
}

// -------------------------------------------------------------------------
// The finding this was built for
// -------------------------------------------------------------------------

/// The credit offsets most of what a career break costs a pension — which is why omitting it
/// made the projection unfairly pessimistic.
///
/// A parent on 3 000 € a month taking two years out loses about 1,4 points of employment
/// entitlement. The Kindererziehungszeit credits back nearly 3,0. For a modest earner the
/// break is not merely softened but **more than covered**, because the credit is pegged to
/// average earnings rather than to their own salary.
///
/// That last clause is the substance: the credit is worth *more* to someone earning below
/// average than the entitlement they gave up. A model without it told those households the
/// opposite.
#[test]
fn the_credit_offsets_most_of_what_a_break_costs_a_pension() {
    let steady = final_points(&household(3_000, &[]), 10);
    let break_only = final_points(
        &household(
            3_000,
            &[Event::UnpaidLeave {
                from_month: 12,
                until_month: Some(36),
            }],
        ),
        10,
    );
    let break_with_child = final_points(
        &household(
            3_000,
            &[
                Event::ChildBorn { month: 11 },
                Event::UnpaidLeave {
                    from_month: 12,
                    until_month: Some(36),
                },
            ],
        ),
        10,
    );

    let cost_of_the_break = steady - break_only;
    assert!(
        cost_of_the_break > 1_000_000,
        "two years out is a real loss"
    );

    // The credit more than covers it for this earner.
    assert!(
        break_with_child > steady,
        "a modest earner should end up ahead: {break_with_child} against {steady}"
    );

    // And the model without the credit was pessimistic by the whole three points.
    assert_eq!(break_with_child - break_only, 2_998_800);
}

/// A household with no birth must be entirely unaffected, so the feature is inert by default.
#[test]
fn a_household_without_a_birth_is_untouched() {
    let plain = household(4_000, &[]);
    let with_other_events = household(
        4_000,
        &[Event::OneOff {
            month: 10,
            amount: euro(-1_000),
        }],
    );
    assert_eq!(
        final_points(&plain, 10),
        final_points(&with_other_events, 10)
    );
}
