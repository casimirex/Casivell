//! Month-by-month household projection over decades.
//!
//! # The architectural constraint that shaped everything here
//!
//! The engine is `#![no_std]`, so there is no allocator and a 480-month timeline cannot
//! be returned as a `Vec`. That looks like an obstacle and is in fact the right design
//! pressure: the kernel **streams**. It computes one month at a time and hands each
//! result to a [`Sink`], keeping no history of its own.
//!
//! Three things follow, all of them improvements over returning a collection:
//!
//! - Memory is `O(1)` in the horizon. A forty-year run holds one month's state.
//! - Monte Carlo becomes natural rather than expensive. Ten thousand paths over forty
//!   years is 4.8 million month-steps, and a sink that keeps running aggregates never
//!   stores a single path.
//! - The caller decides what to keep. A CLI collects into a `Vec`; a chart keeps every
//!   twelfth month; a solver keeps only the final wealth. None of that is the kernel's
//!   business.
//!
//! # What it models
//!
//! Employment income, statutory deductions, expenses, and the wealth that accumulates
//! from the difference — with [`Schedule`] carrying life events that change any of them
//! along the way. Each simulated year resolves its own statutory parameters — via
//! `casivell_projection::resolve`, so enacted law is used where it exists and a labelled
//! projection beyond it — and the tax and contributions for each month are computed by
//! the same verified code that produces a payslip.
//!
//! Pension entitlement accrues alongside, in Entgeltpunkte, so a projection can answer
//! what the household will actually receive rather than only what it saves.
//!
//! # Real versus nominal
//!
//! A forty-year nominal figure is close to meaningless — 4 000 € in 2066 is not 4 000 €
//! today — so [`Basis`] selects whether output is nominal or deflated to the starting
//! year's purchasing power. The deflation happens **in the kernel** and the basis is
//! recorded on every snapshot, because the alternative is a consumer that cannot tell
//! whether a figure has already been deflated, and that is a bug waiting to happen.
//!
//! # What it does not model
//!
//! Deliberately absent, and refused rather than approximated:
//!
//! - **Market variance.** [`monte_carlo()`] takes a caller-supplied set of annual returns
//!   and bootstraps from it. Casivell ships no historical return table, because market
//!   data has its own provenance problem and inventing a plausible series would be
//!   exactly the failure `docs/ROADMAP_ERRATA.md` records.
//! - **Elterngeld.** Modelling it correctly needs the Progressionsvorbehalt of § 32b EStG —
//!   Elterngeld is tax-free but raises the rate on everything else — which in turn needs the
//!   annual assessment inside the kernel rather than monthly withholding. Adding the payment
//!   without the rate effect would understate a family's tax, so it is absent rather than
//!   approximate. [`Event::OtherIncome`] takes a known net amount for anyone who wants to
//!   model it themselves.
//! - **Buying property.** Needs Grunderwerbsteuer by state (3.5 %–6.5 %) and mortgage
//!   amortisation. The deposit and the payment can be modelled today with
//!   [`Event::OneOff`] and [`Event::ExpenseChange`], which is not the same thing.
//! - **The annual assessment.** Tax is computed by withholding, which the statute designs
//!   as an approximation of the annual liability. A refund or a further demand is not
//!   modelled, because determining the taxable income is not implemented.
//! - **Anything but employment income.** No capital income, no self-employment, no
//!   transfers.

#![no_std]
#![forbid(unsafe_code)]
// See the equivalent block in `casivell-core`: panicking constructs are denied in
// library code and permitted only under `cfg(test)`.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::arithmetic_side_effects,
        clippy::indexing_slicing,
    )
)]

// The kernel itself never allocates. Tests do: collecting a whole timeline is exactly
// what a test needs and exactly what the kernel must not do, so `alloc` is pulled in
// under `cfg(test)` only. If this appears outside a test module, the streaming design has
// been abandoned.
#[cfg(test)]
extern crate alloc;

pub mod assessment;
pub mod events;
pub mod household;
pub mod monte_carlo;
pub mod rng;
pub mod timeline;

pub use assessment::{AnnualSettlement, NoAssessment, SETTLEMENT_LAG_MONTHS, filing_status_for};
pub use events::{Event, MonthInputs, Rebase, Schedule};
pub use household::{Household, SimulationConfig};
pub use monte_carlo::{Outcome, monte_carlo};
pub use rng::Prng;
pub use timeline::{Basis, Horizon, MonthSnapshot, SimulationError, Sink, Summary, simulate};
