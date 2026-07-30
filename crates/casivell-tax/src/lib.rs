//! German income tax and its surcharges.
//!
//! # Scope
//!
//! This crate evaluates the tariff of § 32a EStG, the Solidaritätszuschlag under
//! SolzG 1995, and church tax. It contains **no statutory constants**: every
//! figure arrives as a parameter from `casivell-lawdata`. If you find a number
//! like `12_348` in this crate, that is a defect.
//!
//! # What this crate is not
//!
//! It computes *Einkommensteuer* from an already-determined *zu versteuerndes
//! Einkommen*. Determining the zvE — which receipts are taxable, which expenses
//! deductible, how Werbungskosten and Sonderausgaben and außergewöhnliche
//! Belastungen interact — is a far larger problem living in a separate crate, and
//! is where a household simulator earns or loses its credibility. Nothing here
//! should be mistaken for a complete tax calculation.
//!
//! It is also not *Lohnsteuer*. Payroll withholding follows the BMF's
//! Programmablaufplan, a distinct algorithm with its own allowances and
//! rounding, which resembles but does not equal the annual assessment. A
//! simulator that shows a monthly net figure needs the PAP; the two must not be
//! conflated.

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

pub mod church;
pub mod solidarity;
pub mod tariff;

pub use church::{ChurchTaxResult, church_tax};
pub use solidarity::{SurchargeResult, solidarity_surcharge};
pub use tariff::{Assessment, FilingStatus, TariffZone, income_tax};
