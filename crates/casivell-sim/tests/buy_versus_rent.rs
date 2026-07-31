//! Buying against renting, through the same kernel as everything else.
//!
//! # What this can and cannot tell a household
//!
//! It can price the transaction exactly, amortise the loan exactly, and run both futures
//! through the same verified payroll and assessment code. What it cannot do is know how
//! property prices move, and the answer turns on that more than on anything the engine
//! computes.
//!
//! So these tests do not assert that buying wins or loses. They assert the *shape* of the
//! thing: that the assumption is what decides it, that the costs bite when they are supposed
//! to, and that the household's position reconciles. `the_answer_turns_on_the_growth_assumption`
//! is the honest headline — the same household, the same salary, the same house, and the
//! verdict flips on a number nobody knows.

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

fn pct(value: i64) -> Rate {
    Rate::from_percent_millis(value).unwrap()
}

/// A household paying 1 500 € a month in rent, on 6 000 € gross.
fn household(event: Option<Event>) -> Household {
    let insured = Insured::new(35, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
    let employment = Employment::new(
        insured,
        TaxClass::Class1,
        0,
        HealthCover::Statutory {
            supplementary_rate: pct(2_900),
        },
        None,
    )
    .unwrap();
    let mut household = Household::starting_fresh(
        TaxYear::new(2026).unwrap(),
        1,
        employment,
        euro(6_000),
        euro(1_500),
    )
    .unwrap();
    household.initial_wealth = euro(120_000);
    if let Some(event) = event {
        household.schedule = Schedule::new().with(event).unwrap();
    }
    household
}

/// Buying a 400 000 € house in NRW in month 12, 100 000 € down, and thereafter spending
/// 900 € a month on everything that is not the mortgage — Hausgeld, maintenance and the rest.
fn purchase() -> Event {
    Event::PropertyPurchase {
        month: 12,
        price: euro(400_000),
        land: Bundesland::NordrheinWestfalen,
        deposit: euro(100_000),
        agent_rate: pct(3_570),
        interest_rate: pct(3_500),
        repayment_rate: pct(2_000),
        monthly_expenses_after: euro(900),
    }
}

fn config(years: u32, property_growth: i64) -> SimulationConfig {
    let mut config = SimulationConfig::conservative(Horizon::years(years).unwrap(), Basis::Nominal);
    config.investment_return = pct(5_000);
    config.property_growth = pct(property_growth);
    config
}

fn months(household: &Household, config: &SimulationConfig) -> Vec<MonthSnapshot> {
    let mut sink = Collect(Vec::new());
    simulate(household, config, &mut sink).expect("simulates");
    sink.0
}

fn summary(household: &Household, config: &SimulationConfig) -> Summary {
    let mut summary = Summary::default();
    simulate(household, config, &mut summary).expect("simulates");
    summary
}

// -------------------------------------------------------------------------
// The honest headline
// -------------------------------------------------------------------------

/// The verdict flips on the one number nobody knows.
///
/// Same household, same salary, same house, same twenty-five years. At 1 % annual property
/// growth renting wins; at 4 % buying does. Everything the engine computes exactly — the
/// Grunderwerbsteuer, the amortisation, the payroll, the assessment — is identical in both
/// runs. The difference is entirely an assumption.
///
/// That is the finding, and it is why this crate reports the assumption beside the answer
/// rather than issuing a verdict. A tool that picked one growth rate and pronounced would be
/// dressing a guess as a calculation.
#[test]
fn the_answer_turns_on_the_growth_assumption() {
    let verdict = |growth: i64| {
        let buying = summary(&household(Some(purchase())), &config(25, growth)).final_net_worth;
        let renting = summary(&household(None), &config(25, growth)).final_net_worth;
        (buying, renting)
    };

    let (buy_low, rent_low) = verdict(1_000);
    assert!(
        buy_low < rent_low,
        "at 1 % growth renting should win: {buy_low:?} against {rent_low:?}"
    );

    let (buy_high, rent_high) = verdict(4_000);
    assert!(
        buy_high > rent_high,
        "at 4 % growth buying should win: {buy_high:?} against {rent_high:?}"
    );

    // The renter is unaffected by the assumption, which is the control: only the buyer's
    // position moves, so the flip is the property and nothing else.
    assert_eq!(rent_low, rent_high);
}

// -------------------------------------------------------------------------
// The costs, where they bite
// -------------------------------------------------------------------------

/// Completion takes the deposit out of wealth, and the loan carries the rest — including the
/// incidental costs.
///
/// The two readings are the same arithmetic. "The deposit pays the Nebenkosten first and what
/// is left goes against the price" and "the loan is the whole cost less the deposit" differ by
/// nothing: `price + incidentals − deposit` is `price − (deposit − incidentals)`.
///
/// What it means in practice is the thing worth seeing. 100 000 € down on a 400 000 € house
/// leaves a **348 280 € loan against a 400 000 € property** — 87 % of the price, not the 75 %
/// a household that thought of its deposit as a quarter would expect. The 48 280 € of
/// Grunderwerbsteuer, notary and agent bought no equity at all.
#[test]
fn completion_costs_the_deposit_and_the_loan_carries_the_incidentals() {
    let timeline = months(&household(Some(purchase())), &config(3, 2_000));

    // Only the deposit leaves; the month's own savings and return move it a little further.
    let spent = timeline[11].wealth.sub(timeline[12].wealth).unwrap();
    assert!(
        spent > euro(96_000) && spent < euro(101_000),
        "completion moved {spent:?}, not the expected ~100 000 EUR deposit"
    );

    // The property appears the same month, and the loan carries the incidentals too.
    assert_eq!(timeline[11].property_value, Money::ZERO);
    assert!(
        timeline[12].property_value > euro(400_000),
        "already growing"
    );
    let balance = timeline[12].mortgage_balance;
    assert!(
        balance > euro(340_000),
        "the loan must carry the incidentals too, but is only {balance:?}"
    );
    // 87 % of the price, not the 75 % a quarter down would suggest.
    let against_price = balance.cents() * 100 / euro(400_000).cents();
    assert!(
        (85..90).contains(&against_price),
        "the loan is {against_price} % of the price"
    );
}

/// Immediately after completing, the buyer is *worse off* on net worth than the renter — by
/// almost exactly the incidental costs, which bought nothing.
///
/// This is the part households do not price in. The Grunderwerbsteuer, the notary and the
/// agent are money that leaves and does not come back, and it takes years of growth to make
/// it up.
#[test]
fn the_incidental_costs_are_a_hole_that_must_be_grown_out_of() {
    let buying = months(&household(Some(purchase())), &config(3, 2_000));
    let renting = months(&household(None), &config(3, 2_000));

    let gap = renting[12].net_worth.sub(buying[12].net_worth).unwrap();
    assert!(
        gap > euro(35_000) && gap < euro(50_000),
        "the buyer should be about the incidental costs behind, not {gap:?}"
    );
}

// -------------------------------------------------------------------------
// The mortgage, month by month
// -------------------------------------------------------------------------

/// Early on the payment is mostly interest, which is the honest comparison with rent: the
/// repayment part is saving, the interest part is the price of borrowing.
#[test]
fn the_early_payments_are_mostly_interest() {
    let timeline = months(&household(Some(purchase())), &config(5, 2_000));
    let first = &timeline[12];

    assert!(first.mortgage_payment > euro(1_300));
    assert!(
        first.mortgage_interest.cents() * 100 > first.mortgage_payment.cents() * 60,
        "over 60 % of the first payment should be interest"
    );

    // And the interest share falls as the balance does.
    let later = &timeline[59];
    let early_share = first.mortgage_interest.cents() * 1_000 / first.mortgage_payment.cents();
    let later_share = later.mortgage_interest.cents() * 1_000 / later.mortgage_payment.cents();
    assert!(later_share < early_share);
}

/// Nothing is paid before completion, and the balance only ever falls.
#[test]
fn the_mortgage_starts_at_completion_and_only_amortises() {
    let timeline = months(&household(Some(purchase())), &config(10, 2_000));

    for month in &timeline[..12] {
        assert_eq!(month.mortgage_payment, Money::ZERO);
        assert_eq!(month.mortgage_balance, Money::ZERO);
        assert_eq!(month.net_worth, month.wealth, "no property, no difference");
    }

    let mut previous = timeline[12].mortgage_balance;
    for month in &timeline[13..] {
        assert!(
            month.mortgage_balance <= previous,
            "the balance rose at month {}",
            month.month_index
        );
        previous = month.mortgage_balance;
    }
    assert!(previous < timeline[12].mortgage_balance);
}

/// The household's net worth must reconcile: financial wealth plus the property, less the
/// debt. If this drifts, the property is being counted twice or not at all.
#[test]
fn net_worth_reconciles_every_month() {
    for growth in [0_i64, 2_000, 5_000] {
        for month in months(&household(Some(purchase())), &config(15, growth)) {
            assert_eq!(
                month.net_worth,
                month
                    .wealth
                    .add(month.property_value)
                    .unwrap()
                    .sub(month.mortgage_balance)
                    .unwrap(),
                "net worth drifted at month {}",
                month.month_index
            );
        }
    }
}

/// Wealth still moves by exactly its stated parts, with the mortgage now among them.
#[test]
fn wealth_accounts_for_the_mortgage() {
    let timeline = months(&household(Some(purchase())), &config(6, 2_000));
    let mut previous = euro(120_000);
    for month in &timeline {
        // Savings already carry the mortgage payment; completion is the one month with a
        // further deduction, and it is the only month a purchase can occur.
        let expected = previous
            .add(month.investment_return)
            .unwrap()
            .add(month.savings)
            .unwrap()
            .add(month.tax_settlement)
            .unwrap();
        if month.month_index == 12 {
            assert!(month.wealth < expected, "completion must cost money");
        } else {
            assert_eq!(
                month.wealth, expected,
                "wealth drifted at month {}",
                month.month_index
            );
        }
        previous = month.wealth;
    }
}

/// The expenses rebase at completion: rent stops, Hausgeld starts, and the household's own
/// expense growth compounds from the new figure rather than from the rent it no longer pays.
#[test]
fn the_purchase_rebases_the_expense_baseline() {
    let mut household = household(Some(purchase()));
    household.annual_expense_growth = pct(2_000);
    let timeline = months(&household, &config(5, 2_000));

    // Expenses step up on the employment anniversary, which is month 12 — the same month the
    // purchase completes — so month 11 is still the opening rent exactly.
    assert_eq!(timeline[11].expenses, euro(1_500), "still the opening rent");
    assert!(timeline[12].expenses >= euro(900));
    assert!(
        timeline[12].expenses < euro(1_000),
        "now Hausgeld, not rent"
    );
    // And it grows from there rather than snapping back.
    assert!(timeline[47].expenses > timeline[12].expenses);
}

// -------------------------------------------------------------------------
// Inertness
// -------------------------------------------------------------------------

/// A household that buys nothing must be untouched by any of this.
#[test]
fn a_household_that_does_not_buy_is_unaffected() {
    let plain = months(&household(None), &config(10, 5_000));
    for month in &plain {
        assert_eq!(month.property_value, Money::ZERO);
        assert_eq!(month.mortgage_balance, Money::ZERO);
        assert_eq!(month.mortgage_payment, Money::ZERO);
        assert_eq!(month.net_worth, month.wealth);
    }
    // And the growth assumption changes nothing for them.
    let other = summary(&household(None), &config(10, 0));
    assert_eq!(summary(&household(None), &config(10, 5_000)), other);
}

/// The summary must agree with the timeline about the interest paid.
#[test]
fn the_summary_totals_the_mortgage_interest() {
    let h = household(Some(purchase()));
    let c = config(20, 2_000);
    let total: i64 = months(&h, &c)
        .iter()
        .map(|m| m.mortgage_interest.cents())
        .sum();
    assert_eq!(summary(&h, &c).total_mortgage_interest.cents(), total);
    assert!(total > 0);
}
