//! Taxable income under § 2 EStG, and the annual assessment built on it.
//!
//! # The gap this closes
//!
//! Until now Casivell computed tax from an *already-determined* taxable income, and every
//! projection used payroll withholding as a proxy for the annual liability. Determining the
//! taxable income was recorded as the largest remaining correctness risk in the product,
//! because it is where a household simulator earns or loses its credibility: two people on
//! the same salary can owe materially different tax, and the difference lives here.
//!
//! # Verification, and why it is weaker than elsewhere in this repository
//!
//! This is the first substantial piece of Casivell **with no official reference table**.
//! `casivell-payroll` is checked against 516 published values; § 32a is cross-checked
//! against an independent implementation. Neither is available for § 10.
//!
//! That is stated plainly rather than papered over, and the verification is built from what
//! *is* available:
//!
//! 1. **Every constant cited**, as everywhere else, and two of them cross-checked against
//!    the Programmablaufplan's own tables — the Kinderfreibetrag and both Pauschbeträge
//!    appear in each and must agree.
//! 2. **The Altersvorsorge cap is derived**, from the miners' pension ceiling and rate, and
//!    asserted against the published 30 826 €. A derivation that reproduces the published
//!    figure is stronger evidence than transcribing it.
//! 3. **An external validation point for the Günstigerprüfung.** Published commentary puts
//!    the crossover for a jointly assessed couple with one child at roughly 86 000 € of
//!    taxable income under the 2026 tariff. `the_guenstigerpruefung_crossover_matches_published_commentary`
//!    checks it, which tests the comparison logic against a figure derived elsewhere.
//! 4. **A relationship to the Vorsorgepauschale.** Withholding uses a deliberately
//!    simplified allowance; the real deduction computed here should be close to it and
//!    differ in a predictable direction. That is a genuine constraint on both.
//! 5. **Structural properties**: taxable income never exceeds gross, deductions are
//!    monotonic, caps bind where they should, and the Günstigerprüfung never chooses the
//!    worse option.
//!
//! What remains unverified is the exact interaction of § 10 Abs. 3 and Abs. 4 against a
//! real Steuerbescheid. Until someone reconciles it against one, figures from this crate
//! should be treated as a good estimate rather than an exact liability — and
//! [`Assessment::is_exact`] says so in the type rather than in a footnote.
//!
//! # Scope
//!
//! An employee with employment income only. Deliberately absent:
//!
//! - Rental, business, self-employment, agriculture and other income — five of the seven
//!   categories of § 2 Abs. 1. **Capital income is covered**, in [`capital`], because it does
//!   not run through the tariff at all and so is separable from the rest.
//! - **§ 33a**: Unterhaltsleistungen and the Ausbildungsfreibetrag, which turn on the
//!   recipient's own income and assets — facts this crate is not given.
//! - **Riester and Rürup** beyond the statutory pension's own contributions.
//! - Loss carry-forward (§ 10d), the Härteausgleich, and the Altersentlastungsbetrag.
//!
//! Covered, though each arrived after this list was first written: capital income in
//! [`capital`] (§ 32d), the Progressionsvorbehalt in [`progression`] (§ 32b), and
//! außergewöhnliche Belastungen in [`extraordinary`] (§§ 33, 33b).

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

pub mod assessment;
pub mod capital;
pub mod extraordinary;
pub mod progression;
pub mod taxable_income;
pub mod vorsorge;

pub use assessment::{Assessment, AssessmentLaw, ChildRelief, assess};
pub use capital::{CapitalIncomeTax, CapitalRoute, capital_income_tax};
pub use extraordinary::{
    BurdenClaim, ExtraordinaryBurden, extraordinary_burden, reasonable_burden,
};
pub use progression::{Progression, progression_tax};
pub use taxable_income::{Employee, TaxableIncome, taxable_income};
pub use vorsorge::{Contributions, Vorsorgeaufwendungen, vorsorgeaufwendungen};
