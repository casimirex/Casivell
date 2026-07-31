//! Buying a home: the costs of the transaction and the arithmetic of the loan.
//!
//! # Where the certainty stops, and why that matters here more than anywhere else
//!
//! Every other calculation in Casivell is law. This one is mostly not, and the crate is
//! arranged so a reader can see exactly where the boundary falls.
//!
//! **Exact and statutory.** The Grunderwerbsteuer is a rate on a price, set by each Land's own
//! Act. On a 400 000 € house it is 14 000 € in Bayern and 26 000 € in Nordrhein-Westfalen, and
//! that difference is a legal fact.
//!
//! **Exact but not statutory.** The mortgage is arithmetic: an annuity at a stated rate
//! amortises on a schedule that follows from the contract, and [`amortise`] computes it to the
//! cent. Nothing is assumed except the terms the borrower was offered.
//!
//! **Neither.** House price growth, rent growth, maintenance, Hausgeld, what the flat sells
//! for in fifteen years. A buy-versus-rent verdict is dominated by these, and no amount of
//! care in the first two categories makes the third reliable.
//!
//! **This crate deliberately stops before that line.** It prices the transaction and
//! amortises the loan, and does not answer "should I buy". The comparison belongs in
//! `casivell-sim`, where a purchase is an event that swaps rent for a mortgage payment and the
//! household's wealth is projected by the same verified kernel as everything else — with the
//! growth assumptions stated on the page, as every other projection's are.
//!
//! # The German annuity convention
//!
//! A German mortgage is quoted as a *Sollzins* and an *anfängliche Tilgung* — say 3,5 % and
//! 2 % — and the monthly payment is the sum of those applied to the original loan and divided
//! by twelve. The payment then stays fixed while its composition shifts: interest falls as the
//! balance does, so the repayment portion grows, which is why the loan clears far sooner than
//! `100 / 2 = 50` years.
//!
//! The *Zinsbindung* is not the term. A ten-year fix on a thirty-year amortisation leaves a
//! balance to refinance at whatever rates then are, and that residual is the number a
//! household most needs and least often sees. [`Amortisation::remaining_at`] reports it.

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

pub mod mortgage;
pub mod purchase;

pub use mortgage::{Amortisation, MortgageTerms, amortise, monthly_payment};
pub use purchase::{PurchaseCosts, purchase_costs};
