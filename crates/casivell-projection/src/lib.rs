//! Statutory parameters for years no legislature has yet legislated for.
//!
//! # Why this is a separate crate
//!
//! `casivell-lawdata` holds figures transcribed from primary sources and nothing
//! else. That is the property which makes it auditable: a reviewer compares one table
//! against one Gesetzestext. Extrapolation is a *calculation*, and putting it there
//! would mean the crate no longer had the property its whole design rests on.
//!
//! So projection lives here, and the boundary is load-bearing rather than tidy:
//! everything in `casivell-lawdata` is enacted law, everything this crate produces is
//! a forecast, and no figure can be both.
//!
//! # The honesty problem this solves
//!
//! A forty-year household projection must name years far past the last enacted
//! statute. Before this crate, `LawYear::for_year(2027)` returned
//! `YearOutOfRange` — correct, but it meant no projection could run at all.
//!
//! The wrong fix is to widen the verified range. The right one is to make the
//! extrapolation explicit:
//!
//! - It requires [`Assumptions`]. There is no way to obtain a projected year without
//!   naming the growth rates it rests on, so a forecast can never be produced by
//!   accident.
//! - Every parameter set it emits carries [`DataStatus::Projected`], and
//!   `LawYear::status()` already propagates the weakest status of its inputs. A
//!   result computed from a projected year therefore reports itself as projected all
//!   the way up to the UI.
//! - The provenance's `legal_basis` names the assumption and the year it was
//!   projected from, so a figure can be traced to the guess behind it rather than to
//!   a paragraph that does not yet exist.
//!
//! [`DataStatus::Projected`]: casivell_lawdata::DataStatus::Projected
//!
//! # What is projected, and what is held constant
//!
//! | Parameter | Treatment |
//! |---|---|
//! | Grundfreibetrag, tariff Eckwerte, Soli Freigrenze | indexed to **price** inflation |
//! | Contribution ceilings, Durchschnittsentgelt, Bezugsgröße, Rentenwert | indexed to **wage** growth |
//! | § 32a tariff coefficients | **derived** from the projected Eckwerte, not indexed |
//! | The 45 % Reichensteuer threshold | **held constant** — it has not been indexed since 2007 |
//! | Every contribution rate, the Soli rate, the church tax rates | **held constant** |
//!
//! Rates are held constant because they are political decisions with no indexation
//! rule; there is no defensible formula, and inventing one would dress a guess as a
//! method. Holding them constant is a stated assumption a user can reason about.
//!
//! # Why deriving the tariff coefficients matters
//!
//! § 32a's coefficients are not free parameters. The marginal rate is pinned at each
//! zone join — 14 % at the Grundfreibetrag, 23.97 % at the top of zone 2, then 42 %
//! and 45 % — which determines every coefficient from the Eckwerte alone. See
//! [`tariff`] for the algebra.
//!
//! This is what makes a projected tariff credible rather than fabricated: the same
//! derivation, applied to the *enacted* Eckwerte for 2025 and 2026, reproduces all
//! eight published coefficients exactly at the statute's two decimal places. A method
//! that reproduces two enacted years is a reasonable basis for a third.
//!
//! # What this crate does not project
//!
//! `PayrollParameters` — the BMF Programmablaufplan. The PAP is reissued annually as
//! an administrative instrument, and payroll withholding for 2055 is not a question a
//! household projection asks; it wants the annual assessment. Projecting it would
//! invite exactly the false precision this crate exists to avoid.

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

pub mod assumptions;
pub mod growth;
pub mod parameters;
pub mod tariff;

use casivell_core::{MoneyError, TaxYear};
use casivell_lawdata::LawYear;

pub use assumptions::Assumptions;

/// Anything that can prevent a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectionError {
    /// The requested year has enacted data, so projecting it would replace a citable
    /// figure with a guess.
    ///
    /// Callers wanting "enacted if available, otherwise projected" should use
    /// [`resolve`], which is what a simulation kernel needs.
    YearIsEnacted {
        /// The year requested.
        year: u16,
    },
    /// The projection ran so far forward that the tariff's structural invariants
    /// broke — in practice, that the 42 % threshold overtook the unindexed 45 %
    /// threshold.
    ///
    /// Refused rather than returned, because the result would not be a tariff. That
    /// this can happen at all is informative: it says holding the Reichensteuer
    /// threshold constant is untenable over a long enough horizon.
    TariffNoLongerCoherent {
        /// The year at which it broke.
        year: u16,
    },
    /// Arithmetic left the representable domain.
    Arithmetic(MoneyError),
}

impl From<MoneyError> for ProjectionError {
    fn from(error: MoneyError) -> Self {
        Self::Arithmetic(error)
    }
}

impl core::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::YearIsEnacted { year } => write!(
                f,
                "{year} has enacted statutory data; use LawYear::for_year, or resolve() to \
                 accept either"
            ),
            Self::TariffNoLongerCoherent { year } => write!(
                f,
                "projecting to {year} makes the 42 % threshold overtake the unindexed 45 % \
                 threshold, so the tariff would not be well formed"
            ),
            Self::Arithmetic(e) => write!(f, "projection arithmetic failed: {e}"),
        }
    }
}

impl core::error::Error for ProjectionError {}

/// Projects a complete parameter set for a year with no enacted data.
///
/// # Errors
///
/// [`ProjectionError::YearIsEnacted`] if `year` has real data — projecting over a
/// citable figure is always a mistake. [`ProjectionError::TariffNoLongerCoherent`]
/// for horizons long enough to break the tariff's structure.
pub fn project(year: TaxYear, assumptions: &Assumptions) -> Result<LawYear, ProjectionError> {
    if year.has_verified_data() {
        return Err(ProjectionError::YearIsEnacted { year: year.get() });
    }
    let base = LawYear::for_year(TaxYear::LAST_VERIFIED).map_err(ProjectionError::Arithmetic)?;
    let steps = year.years_from(TaxYear::LAST_VERIFIED);

    Ok(LawYear {
        year,
        income_tax: tariff::project_tariff(&base.income_tax, year, steps, assumptions)?,
        social: parameters::project_social(&base.social, year, steps, assumptions)?,
        solidarity: parameters::project_solidarity(&base.solidarity, year, steps, assumptions)?,
        // Church tax rates and the retirement-age table are structural provisions
        // rather than annual figures, so they carry forward unchanged — but with their
        // status downgraded, because asserting that an 8 % rate still holds in 2060 is
        // an assumption however stable the rate has been.
        church_tax: parameters::carry_forward_church_tax(&base.church_tax, year),
        retirement: parameters::carry_forward_retirement(&base.retirement, year),
    })
}

/// Returns enacted parameters where they exist, and a projection otherwise.
///
/// This is the entry point a simulation kernel should call. The returned
/// [`LawYear::status`] distinguishes the two cases, so a caller never has to remember
/// which it got.
///
/// # Errors
///
/// As [`project`], minus [`ProjectionError::YearIsEnacted`], which cannot occur.
pub fn resolve(year: TaxYear, assumptions: &Assumptions) -> Result<LawYear, ProjectionError> {
    if year.has_verified_data() {
        return LawYear::for_year(year).map_err(ProjectionError::Arithmetic);
    }
    project(year, assumptions)
}

#[cfg(test)]
mod tests {
    use super::{Assumptions, ProjectionError, project, resolve};
    use casivell_core::TaxYear;
    use casivell_lawdata::{DataStatus, LawYear};

    fn year(value: u16) -> TaxYear {
        TaxYear::new(value).expect("representable")
    }

    /// The single most important property in the crate: a projected year must never
    /// present itself as enacted law. `LawYear::status()` takes the weakest status of
    /// its parts, so this also proves the marking propagates rather than sitting
    /// unread on one field.
    #[test]
    fn every_projected_year_reports_itself_as_projected() {
        let assumptions = Assumptions::default();
        for value in [2027_u16, 2030, 2040, 2060, 2080, 2095] {
            let law = project(year(value), &assumptions).expect("projects");
            assert_eq!(
                law.status(),
                DataStatus::Projected,
                "{value} must report itself as projected"
            );
            assert!(!law.status().is_binding_law());
            assert_eq!(law.year.get(), value);
        }
    }

    /// Enacted years must never be overwritten by a guess.
    #[test]
    fn projecting_an_enacted_year_is_refused() {
        let assumptions = Assumptions::default();
        for value in [2025_u16, 2026] {
            assert!(matches!(
                project(year(value), &assumptions),
                Err(ProjectionError::YearIsEnacted { .. })
            ));
        }
    }

    /// `resolve` prefers enacted data and falls back to projection, reporting which it
    /// used through the status.
    #[test]
    fn resolve_prefers_enacted_data() {
        let assumptions = Assumptions::default();

        for value in [2025_u16, 2026] {
            let resolved = resolve(year(value), &assumptions).expect("resolves");
            let enacted = LawYear::for_year(year(value)).expect("enacted");
            assert_eq!(resolved.status(), DataStatus::Enacted);
            assert_eq!(
                resolved.income_tax, enacted.income_tax,
                "{value} must come back byte-identical to the enacted table"
            );
        }

        let projected = resolve(year(2035), &assumptions).expect("resolves");
        assert_eq!(projected.status(), DataStatus::Projected);
    }

    /// Every year up to the coherence horizon must resolve, so a projection cannot fail
    /// part-way through a run that started successfully.
    ///
    /// The horizon exists because the 45 % threshold is not indexed: at the default 2 %
    /// inflation the 42 % threshold overtakes it in **2096**, seventy years out, and the
    /// tariff stops being well formed. Pinning the exact year documents the limit rather
    /// than pretending there is none — and the limit is itself a finding, since it says
    /// leaving the Reichensteuer threshold frozen cannot hold indefinitely.
    ///
    /// Well beyond any household projection: a forty-year forecast reaches 2066.
    #[test]
    fn every_year_up_to_the_coherence_horizon_resolves() {
        const FIRST_INCOHERENT_YEAR: u16 = 2096;

        let assumptions = Assumptions::default();
        let mut value = TaxYear::FIRST_VERIFIED.get();
        while value < FIRST_INCOHERENT_YEAR {
            let result = resolve(year(value), &assumptions);
            assert!(result.is_ok(), "{value} failed to resolve: {result:?}");
            value = value.saturating_add(1);
        }

        // The horizon is exactly where it is claimed to be: the year before resolves and
        // this one does not. A one-sided assertion would drift silently.
        assert!(
            resolve(year(FIRST_INCOHERENT_YEAR.saturating_sub(1)), &assumptions).is_ok(),
            "the year before the horizon should still resolve"
        );
        assert!(
            matches!(
                resolve(year(FIRST_INCOHERENT_YEAR), &assumptions),
                Err(ProjectionError::TariffNoLongerCoherent { .. })
            ),
            "the horizon should be refused, not returned"
        );
    }

    /// A forty-year projection — the horizon the product actually promises — must run
    /// end to end without a single year failing.
    #[test]
    fn a_forty_year_projection_runs_end_to_end() {
        let assumptions = Assumptions::default();
        let mut previous_allowance = 0_i64;
        for offset in 0_u16..=40 {
            let value = TaxYear::LAST_VERIFIED.get().saturating_add(offset);
            let law = resolve(year(value), &assumptions).expect("resolves");
            let allowance = law.income_tax.basic_allowance_euro;
            assert!(
                allowance >= previous_allowance,
                "the Grundfreibetrag fell in {value}"
            );
            previous_allowance = allowance;
        }
        // Forty years at 2 % is about 2.21x, so 12 348 EUR should reach roughly 27 000 EUR.
        assert!(
            (26_000..=28_500).contains(&previous_allowance),
            "the 2066 Grundfreibetrag came out at {previous_allowance}"
        );
    }

    /// A projection is only as good as its assumption, so the assumption must be
    /// recorded where a reader will find it.
    #[test]
    fn the_provenance_names_the_assumption_and_the_base_year() {
        let assumptions = Assumptions::default();
        let law = project(year(2040), &assumptions).expect("projects");
        let basis = law.income_tax.provenance.legal_basis;
        assert!(
            basis.contains("projected"),
            "the basis must say it is projected: {basis}"
        );
        assert!(
            basis.contains("2026"),
            "the basis must name the year projected from: {basis}"
        );
        assert!(
            !law.income_tax.provenance.status.is_binding_law(),
            "a projected tariff must not claim to be binding law"
        );
    }
}
