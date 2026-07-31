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

pub mod benefits;
pub mod deductions;
pub mod extraordinary;
pub mod fingerprint;
pub mod income_tax;
pub mod payroll;
pub mod property;
pub mod provenance;
pub mod retirement;
pub mod social;
pub mod surcharges;

pub use benefits::ElterngeldParameters;
pub use deductions::DeductionParameters;
pub use extraordinary::{BurdenRates, BurdenRow, ExtraordinaryBurdenParameters};
pub use fingerprint::{Fingerprint, Fingerprinted};
pub use income_tax::{IncomeTaxTariff, ProgressionZone, ProportionalZone};
pub use payroll::{PayrollParameters, TaxClass};
pub use property::PropertyCostParameters;
pub use provenance::{DataStatus, Provenance};
pub use retirement::{MONTHS_PER_YEAR, RetirementParameters, retirement_age_months};
pub use social::{
    CareInsurance, HealthInsurance, PensionInsurance, SocialParameters, UnemploymentInsurance,
};
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
    /// Retirement age and Zugangsfaktor parameters, SGB VI.
    pub retirement: RetirementParameters,
    /// Deductions between gross pay and taxable income, §§ 9a, 10, 32 Abs. 6, 66 EStG.
    pub deductions: DeductionParameters,
    /// Elterngeld parameters, BEEG.
    pub benefits: ElterngeldParameters,
    /// Außergewöhnliche Belastungen, §§ 33 and 33b EStG.
    pub burden: ExtraordinaryBurdenParameters,
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
        let retirement = match RetirementParameters::for_year(year) {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        let deductions = match DeductionParameters::for_year(year) {
            Ok(d) => d,
            Err(e) => return Err(e),
        };
        let benefits = match ElterngeldParameters::for_year(year) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        let burden = match ExtraordinaryBurdenParameters::for_year(year) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        Ok(Self {
            year,
            income_tax,
            social,
            solidarity,
            church_tax,
            retirement,
            deductions,
            benefits,
            burden,
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
            .weakest(self.retirement.provenance.status)
    }
}

#[cfg(test)]
mod tests {
    use super::LawYear;
    use casivell_core::TaxYear;

    /// Every year in the declared support range must actually resolve. Without
    /// this, widening `TaxYear::LAST_VERIFIED` without adding data would fail at runtime
    /// instead of at `cargo test`.
    #[test]
    fn every_supported_year_resolves() {
        let mut year = TaxYear::FIRST_VERIFIED.get();
        while year <= TaxYear::LAST_VERIFIED.get() {
            let tax_year = TaxYear::new(year).expect("within the declared range");
            assert!(
                LawYear::for_year(tax_year).is_ok(),
                "TaxYear::FIRST_VERIFIED..=MAX claims {year} is supported but no data exists"
            );
            year = year.saturating_add(1);
        }
    }

    #[test]
    fn unsupported_years_are_refused_rather_than_approximated() {
        // Before the first transcribed statute there is nothing to extrapolate from,
        // so the year itself is refused.
        assert!(TaxYear::new(TaxYear::FIRST_VERIFIED.get().saturating_sub(1)).is_err());

        // *After* the last verified year the guard sits on the data lookup, not on
        // year construction. The year is representable — a projection has to be able
        // to name it — but `LawYear::for_year` still refuses to hand back figures it
        // cannot cite. This is the invariant that matters, and it is the one thing
        // `casivell-projection` must not be able to weaken.
        let unverified = TaxYear::new(TaxYear::LAST_VERIFIED.get().saturating_add(1))
            .expect("a year past the verified range is still representable");
        assert!(!unverified.has_verified_data());
        assert!(
            LawYear::for_year(unverified).is_err(),
            "enacted parameters must never be returned for an unverified year"
        );
    }

    /// Every year the type will represent must either have verified data or report
    /// that it does not. There is no third state, and nothing in between the two
    /// bounds may be silently unbacked.
    #[test]
    fn verified_data_exists_for_exactly_the_years_that_claim_it() {
        let mut year = TaxYear::FIRST_VERIFIED.get();
        while year <= TaxYear::LAST_REPRESENTABLE.get() {
            let tax_year = TaxYear::new(year).expect("representable");
            assert_eq!(
                tax_year.has_verified_data(),
                LawYear::for_year(tax_year).is_ok(),
                "{year}: has_verified_data disagrees with whether data resolves"
            );
            // Step coarsely once past the verified range; the interesting boundary is
            // the first few years.
            year = year.saturating_add(if year < TaxYear::LAST_VERIFIED.get() + 3 {
                1
            } else {
                7
            });
        }
    }
}
