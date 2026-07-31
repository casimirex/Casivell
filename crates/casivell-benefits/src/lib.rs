//! Elterngeld under the BEEG.
//!
//! # Why this is a crate of its own
//!
//! § 2e BEEG computes Elterngeld's tax deduction with the *Programmablaufplan*. That puts this
//! calculation above `casivell-payroll` in the dependency order, so it cannot live in
//! `casivell-social` — which payroll already depends on — without a cycle. The layering is
//! the statute's, not a preference.
//!
//! It is also a happy accident. The largest single deduction in the Elterngeld formula runs
//! through the best-verified code in this repository: the PAP, checked against 516 official
//! BMF values. A benefit with no published reference table inherits the verification of one
//! that has one.
//!
//! # What the calculation actually is
//!
//! Elterngeld replaces a share of a *stylised* pre-birth net income, and both halves of that
//! sentence carry weight.
//!
//! ```text
//!   monthly gross over the twelve months before the birth      § 2c Abs. 1
//!   − one twelfth of the Arbeitnehmer-Pauschbetrag             § 2c Abs. 1
//!   − tax, from the Programmablaufplan                         § 2e
//!   − 21 % flat for social insurance, with no ceilings         § 2f
//!   = Elterngeld-Netto
//!   × a rate between 65 % and 100 %, sliding with that net     § 2 Abs. 1–2
//!   = Elterngeld, clamped to 300 … 1 800 €                     § 2 Abs. 1, 4
//! ```
//!
//! The stylised net is not the payslip net and is not meant to be. § 2f Abs. 3 disregards the
//! Beitragsbemessungsgrenzen outright, so a high earner's notional social deduction is far
//! larger than the one they actually paid. That is the statute simplifying, not this crate.
//!
//! # The part people get wrong
//!
//! Elterngeld is **tax-free but not free**: § 32b EStG adds it to the rate base, so it raises
//! the tax on every other euro the household earns that year. The demand arrives with the
//! Steuerbescheid, months after the money has been spent. `casivell_income::progression_tax`
//! computes that side, and a projection that showed one without the other would be telling
//! only the good half of the story.
//!
//! # Scope
//!
//! **Entitlement is asserted by the caller, not decided here.** Residence, the child living in
//! the household, working hours during the reference period — these are facts a household
//! simulator does not hold. What is computed is *how much*, given that someone qualifies, plus
//! the one eligibility rule that is purely arithmetic: the § 1 Abs. 8 income limit.
//!
//! Not modelled: self-employment income (§ 2d), the Bemessungszeitraum shifts of § 2b for
//! earlier parental leave or illness, Mutterschaftsgeld offsetting, and the Partnerschaftsbonus
//! of § 4b.

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

pub mod elterngeld;

pub use elterngeld::{
    Elterngeld, ElterngeldRequest, Variant, elterngeld, elterngeld_netto, replacement_rate,
};
