//! Year-keyed German statutory parameters, each carrying its own provenance.
//!
//! # The rule this crate exists to enforce
//!
//! **No statutory number appears anywhere else in the engine.** Calculation
//! crates take a parameter struct as an argument; they never contain a literal
//! like `12_348`. A reviewer checking whether Casivell computes 2026 tax
//! correctly reads one table here and compares it against one Gesetzestext,
//! rather than grepping for magic numbers across a simulation kernel.
//!
//! # Why provenance is mandatory, not decoration
//!
//! Three separate product requirements collapse into it:
//!
//! 1. **Auditability.** The claim is "German law as code, community-auditable".
//!    That is only true if every figure names the paragraph it came from. Each
//!    parameter set therefore carries a [`Provenance`], and there is no
//!    constructor that omits it.
//! 2. **Reproducibility.** A scenario saved in 2026 must still produce its
//!    original numbers when reopened in 2031, or the saved projection silently
//!    changes under the user. Parameter sets are immutable and keyed by year;
//!    adding 2027 never mutates 2026.
//! 3. **Honesty about the future.** A 40-year projection necessarily runs past
//!    the last enacted statute. [`DataStatus`] marks each set as enacted,
//!    draft, or projected, so the UI can say which parts of a forecast are law
//!    and which are assumption. The original specification had no such
//!    distinction and would have presented a guess about 2059 in the same
//!    typeface as § 32a.
//!
//! # Units
//!
//! Money is [`Money`], rates are [`Rate`]. Tariff coefficients are the one
//! exception: they are stored as scaled integers in the exact form the statute
//! prints them, so that a reviewer can diff the struct against the Gesetzestext
//! line by line. See [`income_tax::ProgressionZone`].
//!
//! [`Money`]: casivell_core::Money
//! [`Rate`]: casivell_core::Rate

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

pub mod income_tax;
pub mod provenance;
pub mod social;
pub mod surcharges;

pub use income_tax::{IncomeTaxTariff, ProgressionZone, ProportionalZone};
pub use provenance::{DataStatus, Provenance};
pub use social::{CareInsurance, HealthInsurance, PensionInsurance, SocialParameters};
pub use surcharges::{Bundesland, ChurchTaxParameters, SolidarityParameters};

use casivell_core::{MoneyError, TaxYear};

/// Every statutory parameter Casivell needs for one calendar year.
///
/// Bundled into a single struct so that a simulation resolves its law once per
/// year and then computes without further lookups — the hot loop must not be
/// doing table searches.
#[derive(Debug, Clone, Copy)]
pub struct LawYear {
    /// The year these parameters apply to.
    pub year: TaxYear,
    /// Income tax tariff, § 32a EStG.
    pub income_tax: IncomeTaxTariff,
    /// Social insurance contribution parameters.
    pub social: SocialParameters,
    /// Solidaritätszuschlag parameters, SolzG 1995.
    pub solidarity: SolidarityParameters,
    /// Church tax rates.
    pub church_tax: ChurchTaxParameters,
}

impl LawYear {
    /// Resolves the complete parameter set for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] when no verified set exists. Callers must
    /// treat this as "we cannot answer", never as "use the nearest year" — the
    /// substitution would be invisible in the output.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        let income_tax = match IncomeTaxTariff::for_year(year) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };
        let social = match SocialParameters::for_year(year) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        let solidarity = match SolidarityParameters::for_year(year) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        let church_tax = match ChurchTaxParameters::for_year(year) {
            Ok(c) => c,
            Err(e) => return Err(e),
        };
        Ok(Self {
            year,
            income_tax,
            social,
            solidarity,
            church_tax,
        })
    }

    /// The weakest [`DataStatus`] among this year's parameter sets.
    ///
    /// A projection is only as trustworthy as its least certain input, so the UI
    /// labels a result with this rather than with any individual figure's status.
    #[must_use]
    pub const fn status(&self) -> DataStatus {
        self.income_tax
            .provenance
            .status
            .weakest(self.social.status())
            .weakest(self.solidarity.provenance.status)
            .weakest(self.church_tax.provenance.status)
    }
}

#[cfg(test)]
mod tests {
    use super::LawYear;
    use casivell_core::TaxYear;

    /// Every year in the declared support range must actually resolve. Without
    /// this, widening `TaxYear::MAX` without adding data would fail at runtime
    /// instead of at `cargo test`.
    #[test]
    fn every_supported_year_resolves() {
        let mut year = TaxYear::MIN.get();
        while year <= TaxYear::MAX.get() {
            let tax_year = TaxYear::new(year).expect("within the declared range");
            assert!(
                LawYear::for_year(tax_year).is_ok(),
                "TaxYear::MIN..=MAX claims {year} is supported but no data exists"
            );
            year = year.saturating_add(1);
        }
    }

    #[test]
    fn unsupported_years_are_refused_rather_than_approximated() {
        assert!(TaxYear::new(TaxYear::MIN.get().saturating_sub(1)).is_err());
        assert!(TaxYear::new(TaxYear::MAX.get().saturating_add(1)).is_err());
    }
}
