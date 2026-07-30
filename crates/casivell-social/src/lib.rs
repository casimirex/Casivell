//! German social insurance: contributions and statutory pension entitlement.
//!
//! # Scope
//!
//! Two things, both under SGB III/V/VI/XI:
//!
//! - [`mod@contributions`] splits the four branches of social insurance between
//!   employee and employer for a given monthly gross salary.
//! - [`mod@pension`] converts contributory income into Entgeltpunkte and
//!   Entgeltpunkte into a monthly pension.
//!
//! As with `casivell-tax`, this crate contains **no statutory constants**. Every
//! figure arrives as a parameter from `casivell-lawdata`.
//!
//! # What this crate is not
//!
//! It is not a payroll calculation. Deriving net pay additionally requires
//! *Lohnsteuer*, which follows the BMF Programmablaufplan — a distinct algorithm
//! with its own allowances, its own Vorsorgepauschale, and its own rounding, which
//! resembles but does not equal the annual assessment in `casivell-tax`. The
//! contributions computed here are one input to that, not the whole of it.
//!
//! It also models an ordinary employee. Minijobs, the Übergangsbereich
//! (Midijob) sliding scale, voluntary and self-employed contributions, civil
//! servants, and the Künstlersozialkasse each follow different rules and are not
//! implemented. [`contributions::contributions`] is documented as applying to
//! employment above the Übergangsbereich; feeding it a Minijob salary produces a
//! figure that is arithmetically consistent and legally wrong.

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

pub mod contributions;
pub mod pension;

pub use contributions::{ContributionSplit, Insured, SocialContributions, contributions};
pub use pension::{EntgeltPoints, monthly_pension, zugangsfaktor};
