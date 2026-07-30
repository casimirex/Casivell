//! German payroll withholding: Lohnsteuer, Solidaritätszuschlag, church tax, and
//! net pay.
//!
//! # Why this is a separate algorithm
//!
//! Lohnsteuer is **not** the annual assessment in `casivell-tax` applied to a
//! monthly figure. It is the BMF *Programmablaufplan für die maschinelle Berechnung
//! der Lohnsteuer*, a distinct algorithm with its own allowances, its own
//! Vorsorgepauschale, its own tax-class formulas, and its own rounding points. It
//! shares only the § 32a tariff with the annual assessment.
//!
//! Approximating it — annualising the salary and applying § 32a — gets a monthly
//! net figure wrong by tens of euros, because it omits the Vorsorgepauschale
//! entirely. That is a large error presented with the confidence of an exact one,
//! which is the failure mode `docs/ROADMAP_ERRATA.md` exists to prevent. So this
//! crate implements the PAP, and is checked against the PAP's own published
//! Prüftabelle: 516 official reference values across six tax classes.
//!
//! # Fidelity, and one deliberate departure
//!
//! The PAP states in § 2.2 that *"bei der Steuerberechnung werden Gleitkommafelder
//! verwendet"* — the reference algorithm uses floating point. Casivell implements
//! its **semantics** in exact integers instead.
//!
//! This is a considered choice, not an oversight. The PAP mandates truncation to
//! whole euro or whole cent at defined points, and every field's precision is
//! declared, so the algorithm is fully specified over the rationals; the floats are
//! an artefact of the reference implementation. Where the two could differ is when
//! float error carries a value across a rounding boundary — and there the integer
//! result is the correct one and the float result the artefact. All 516 Prüftabelle
//! values agree exactly, so no such divergence occurs in the checked range.
//!
//! What this does mean: bit-exact agreement with one particular payroll product is
//! not guaranteed at the sub-cent boundary. Agreement with the *statute* is.
//!
//! # Scope
//!
//! Implemented: tax classes I–VI, annual and monthly pay periods, the
//! Vorsorgepauschale in full (pension, health, care, unemployment, and the 1 900 €
//! cap), statutory and private health cover, Kinderfreibeträge for the
//! Solidaritätszuschlag and church tax base, the Saxon care split, and the
//! childless surcharge.
//!
//! Not implemented, and refused rather than approximated:
//!
//! - **Weekly and daily pay periods.** The PAP's `LZZ = 3` and `4` scale by
//!   `360/7` and `1/360`, which do not terminate in decimal. Supporting them
//!   exactly needs a finer scale than cents; supporting them approximately would
//!   silently disagree with payroll. Salaried employment is monthly or annual.
//! - **Versorgungsbezüge** (`VBEZ`) and the Altersentlastungsbetrag (`ALTER1`),
//!   which matter for pensions run through payroll.
//! - **Sonstige Bezüge** (`SONSTB`): one-off payments such as a thirteenth month
//!   or a bonus follow § 39b Abs. 3, a separate calculation.
//! - **The Faktorverfahren** (`F`), an alternative to classes III/V for couples.
//! - **ELStAM Freibetrag/Hinzurechnungsbetrag** beyond the plain annual amounts
//!   this crate accepts.

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

pub mod net;
pub mod withholding;

pub use net::{NetPay, monthly_net, net_pay};
pub use withholding::{
    CareStatus, Employment, HealthCover, PayPeriod, PayrollLaw, Withholding, withhold,
};
