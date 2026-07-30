//! Projecting social insurance and the surcharges.
//!
//! # What moves with wages, and what does not move at all
//!
//! Every figure the SVBezGrV sets annually — the two contribution ceilings, the
//! Durchschnittsentgelt, the Bezugsgröße, the Jahresarbeitsentgeltgrenze — derives from
//! average earnings, so all of them are indexed to wage growth. The Rentenwert follows
//! wages too, though for a different reason: § 68 SGB VI ties it to a Lohnkomponente
//! modified by a Nachhaltigkeitsfaktor and a Niveauschutzklausel. Reducing that to
//! plain wage growth is a simplification, and a stated one.
//!
//! Every *rate* is held constant. There is no indexation rule for a contribution rate:
//! 18.6 % for pensions, 14.6 % for health, 3.6 % for care, 5.5 % for the
//! Solidaritätszuschlag, 8 % or 9 % for church tax — each is a political decision. A
//! formula would dress a guess as a method, so they carry forward unchanged and the
//! provenance says so.
//!
//! The Soli Freigrenze is indexed to *prices* rather than wages: it moves with the
//! income tax tariff, which the Steuerfortentwicklungsgesetz adjusts for inflation.

use casivell_core::{Money, TaxYear};
use casivell_lawdata::{
    CareInsurance, ChurchTaxParameters, DataStatus, HealthInsurance, PensionInsurance, Provenance,
    RetirementParameters, SocialParameters, SolidarityParameters, UnemploymentInsurance,
};

use crate::ProjectionError;
use crate::assumptions::Assumptions;
use crate::growth::{compound_to_cent, compound_to_euro, compound_to_multiple};

/// § 159 SGB VI rounds the annual pension ceiling to a multiple of 600 €.
///
/// Verifiable against the enacted figure: 101 400 = 169 × 600.
const PENSION_CEILING_GRID_EURO: i64 = 600;

/// § 6 Abs. 7 SGB V rounds the annual health ceiling to a multiple of 450 €.
///
/// Verifiable against the enacted figure: 69 750 = 155 × 450.
const HEALTH_CEILING_GRID_EURO: i64 = 450;

/// Months in a year, for converting between the monthly and annual forms the statute
/// uses for different ceilings.
const MONTHS: i64 = 12;

/// Builds the provenance every projected parameter set carries.
///
/// The `legal_basis` names the assumptions and the year projected from, so a figure can
/// be traced to the guess behind it. It deliberately does *not* cite a paragraph: there
/// is no paragraph, and pretending otherwise is the failure this crate exists to avoid.
#[must_use]
pub(crate) fn projected_provenance(
    what: &'static str,
    source_url: &'static str,
    assumptions: &Assumptions,
) -> Provenance {
    // `Provenance` holds `&'static str`, so the assumption cannot be formatted into it
    // without allocating — and this crate is `no_std`. The rates are therefore
    // described qualitatively here and carried numerically by the caller's
    // `Assumptions`, which a UI displays alongside any projected figure.
    let basis = if assumptions.price_inflation().is_zero() && assumptions.wage_growth().is_zero() {
        concat_basis(what, "frozen assumptions, projected from 2026")
    } else {
        concat_basis(what, "projected from 2026 under stated growth assumptions")
    };
    Provenance::new(basis, source_url, "2026-07-30", DataStatus::Projected)
}

/// Joins a description and a note without allocating.
///
/// `Provenance` requires `&'static str`, so the combinations are enumerated rather than
/// formatted. There are few enough of them that this is clearer than introducing an
/// owned string type into a `no_std` crate.
fn concat_basis(what: &'static str, note: &'static str) -> &'static str {
    match (what, note) {
        ("§ 32a EStG, projected", n) if n.starts_with("frozen") => {
            "§ 32a EStG, projected from 2026 under frozen assumptions (NOT enacted law)"
        }
        ("§ 32a EStG, projected", _) => {
            "§ 32a EStG, projected from 2026 under stated growth assumptions (NOT enacted law)"
        }
        ("SGB III/V/VI/XI, projected", _) => {
            "SGB III/V/VI/XI Rechengrößen, projected from 2026 under stated wage growth (NOT enacted law)"
        }
        ("SolzG 1995, projected", _) => {
            "§ 3 SolzG 1995, projected from 2026 under stated price inflation (NOT enacted law)"
        }
        ("church tax, carried forward", _) => {
            "Kirchensteuergesetze der Länder, rates assumed unchanged from 2026 (NOT enacted law)"
        }
        ("retirement, carried forward", _) => {
            "§§ 77, 235 SGB VI, assumed unchanged from 2026 (NOT enacted law)"
        }
        (other, _) => other,
    }
}

/// Projects the social insurance parameters.
///
/// # Errors
///
/// [`ProjectionError::Arithmetic`] on a domain violation.
pub(crate) fn project_social(
    base: &SocialParameters,
    year: TaxYear,
    steps: u32,
    assumptions: &Assumptions,
) -> Result<SocialParameters, ProjectionError> {
    let wages = assumptions.wage_growth();
    let provenance = projected_provenance(
        "SGB III/V/VI/XI, projected",
        base.pension.provenance.source_url,
        assumptions,
    );

    // Ceilings are stated monthly but rounded on an annual grid, so index the annual
    // figure, snap it, and convert back. Doing it the other way round would land the
    // monthly amount off the statutory grid.
    let pension_ceiling = project_ceiling(
        base.pension.ceiling_monthly,
        wages,
        steps,
        PENSION_CEILING_GRID_EURO,
    )?;
    let health_ceiling = project_ceiling(
        base.health.ceiling_monthly,
        wages,
        steps,
        HEALTH_CEILING_GRID_EURO,
    )?;

    Ok(SocialParameters {
        year,
        pension: PensionInsurance {
            contribution_rate: base.pension.contribution_rate,
            ceiling_monthly: pension_ceiling,
            average_earnings_annual: grow_euro(base.pension.average_earnings_annual, wages, steps)?,
            // The Rentenwert changes on 1 July, so a calendar year holds two values and
            // the second half of one year is the first half of the next. Projecting both
            // from the base year's second-half value preserves that continuity by
            // construction — a projection that broke it would fail the same invariant
            // test the enacted tables pass.
            pension_value_jan_to_jun: grow_cent(
                base.pension.pension_value_jul_to_dec,
                wages,
                steps.saturating_sub(1),
            )?,
            pension_value_jul_to_dec: grow_cent(
                base.pension.pension_value_jul_to_dec,
                wages,
                steps,
            )?,
            provenance,
        },
        unemployment: UnemploymentInsurance {
            contribution_rate: base.unemployment.contribution_rate,
            // Shares the pension ceiling, as it has in every SVBezGrV.
            ceiling_monthly: pension_ceiling,
            provenance,
        },
        health: HealthInsurance {
            general_rate: base.health.general_rate,
            average_supplementary_rate: base.health.average_supplementary_rate,
            ceiling_monthly: health_ceiling,
            compulsory_insurance_threshold_annual: grow_euro(
                base.health.compulsory_insurance_threshold_annual,
                wages,
                steps,
            )?,
            provenance,
        },
        // Care rates and the age and child thresholds are all structural.
        care: CareInsurance {
            provenance,
            ..base.care
        },
        reference_value_monthly: grow_euro(base.reference_value_monthly, wages, steps)?,
    })
}

/// Projects the Solidaritätszuschlag parameters.
///
/// # Errors
///
/// [`ProjectionError::Arithmetic`] on a domain violation.
pub(crate) fn project_solidarity(
    base: &SolidarityParameters,
    year: TaxYear,
    steps: u32,
    assumptions: &Assumptions,
) -> Result<SolidarityParameters, ProjectionError> {
    let prices = assumptions.price_inflation();
    let individual = grow_euro(base.exemption_individual, prices, steps)?;
    Ok(SolidarityParameters {
        year,
        rate: base.rate,
        exemption_individual: individual,
        // § 3 Abs. 3 SolzG sets the joint Freigrenze at exactly twice the individual
        // one, so it is derived rather than indexed separately — otherwise rounding
        // could break the doubling the enacted tables are tested for.
        exemption_joint: individual.mul_int(2).map_err(ProjectionError::Arithmetic)?,
        taper_rate: base.taper_rate,
        provenance: projected_provenance(
            "SolzG 1995, projected",
            base.provenance.source_url,
            assumptions,
        ),
    })
}

/// Carries the church tax rates forward with their status downgraded.
///
/// The rates have been 8 % and 9 % for decades, but asserting they still hold in 2060 is
/// an assumption however stable they have been, so the status changes even though the
/// figures do not.
#[must_use]
pub(crate) fn carry_forward_church_tax(
    base: &ChurchTaxParameters,
    year: TaxYear,
) -> ChurchTaxParameters {
    ChurchTaxParameters {
        year,
        provenance: Provenance::new(
            concat_basis("church tax, carried forward", ""),
            base.provenance.source_url,
            base.provenance.verified_on,
            DataStatus::Projected,
        ),
        ..*base
    }
}

/// Carries the retirement parameters forward with their status downgraded.
///
/// The Zugangsfaktor rates and the § 235 retirement-age table are structural, but the
/// Regelaltersgrenze is politically live — proposals to raise it past 67 recur — so a
/// projection must not present it as settled.
#[must_use]
pub(crate) fn carry_forward_retirement(
    base: &RetirementParameters,
    year: TaxYear,
) -> RetirementParameters {
    RetirementParameters {
        year,
        provenance: Provenance::new(
            concat_basis("retirement, carried forward", ""),
            base.provenance.source_url,
            base.provenance.verified_on,
            DataStatus::Projected,
        ),
        ..*base
    }
}

/// Indexes a whole-euro amount forward.
fn grow_euro(base: Money, rate: casivell_core::Rate, steps: u32) -> Result<Money, ProjectionError> {
    compound_to_euro(base, rate, steps).map_err(ProjectionError::Arithmetic)
}

/// Indexes a cent-precision amount forward.
fn grow_cent(base: Money, rate: casivell_core::Rate, steps: u32) -> Result<Money, ProjectionError> {
    compound_to_cent(base, rate, steps).map_err(ProjectionError::Arithmetic)
}

/// Indexes a monthly ceiling, snapping the annual figure to its statutory grid.
fn project_ceiling(
    monthly: Money,
    rate: casivell_core::Rate,
    steps: u32,
    grid_euro: i64,
) -> Result<Money, ProjectionError> {
    let annual = monthly
        .mul_int(MONTHS)
        .map_err(ProjectionError::Arithmetic)?;
    let grown = compound_to_multiple(annual, rate, steps, grid_euro)
        .map_err(ProjectionError::Arithmetic)?;
    grown
        .div_int(MONTHS, casivell_core::Rounding::Floor)
        .map_err(ProjectionError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::{
        HEALTH_CEILING_GRID_EURO, PENSION_CEILING_GRID_EURO, carry_forward_church_tax,
        carry_forward_retirement, project_social, project_solidarity,
    };
    use crate::assumptions::Assumptions;
    use casivell_core::TaxYear;
    use casivell_lawdata::{
        ChurchTaxParameters, RetirementParameters, SocialParameters, SolidarityParameters,
    };

    fn year(value: u16) -> TaxYear {
        TaxYear::new(value).expect("representable")
    }

    /// The year `steps` past the last verified one. Built from the base year rather
    /// than by casting the step count, so no narrowing conversion is needed.
    fn projected_year(steps: u32) -> TaxYear {
        let offset = u16::try_from(steps).expect("a step count within a century");
        year(TaxYear::LAST_VERIFIED.get().saturating_add(offset))
    }

    fn base_social() -> SocialParameters {
        SocialParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted")
    }

    fn base_solidarity() -> SolidarityParameters {
        SolidarityParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted")
    }

    /// Frozen assumptions must change no figure, so projection contributes nothing of
    /// its own when asked to change nothing.
    #[test]
    fn frozen_assumptions_reproduce_the_base_figures() {
        let base = base_social();
        let projected =
            project_social(&base, year(2050), 24, &Assumptions::frozen()).expect("projects");

        assert_eq!(
            projected.pension.ceiling_monthly,
            base.pension.ceiling_monthly
        );
        assert_eq!(
            projected.pension.average_earnings_annual,
            base.pension.average_earnings_annual
        );
        assert_eq!(
            projected.health.ceiling_monthly,
            base.health.ceiling_monthly
        );
        assert_eq!(
            projected.reference_value_monthly,
            base.reference_value_monthly
        );
        assert_eq!(
            projected.pension.pension_value_jul_to_dec,
            base.pension.pension_value_jul_to_dec
        );
    }

    /// Every rate must survive a projection untouched. A drifting contribution rate
    /// would be an invented policy change.
    #[test]
    fn every_rate_is_held_constant() {
        let base = base_social();
        let projected =
            project_social(&base, year(2070), 44, &Assumptions::default()).expect("projects");

        assert_eq!(
            projected.pension.contribution_rate,
            base.pension.contribution_rate
        );
        assert_eq!(
            projected.unemployment.contribution_rate,
            base.unemployment.contribution_rate
        );
        assert_eq!(projected.health.general_rate, base.health.general_rate);
        assert_eq!(
            projected.health.average_supplementary_rate,
            base.health.average_supplementary_rate
        );
        assert_eq!(projected.care.base_rate, base.care.base_rate);
        assert_eq!(
            projected.care.childless_surcharge,
            base.care.childless_surcharge
        );
        assert_eq!(
            projected.care.saxony_employee_surcharge,
            base.care.saxony_employee_surcharge
        );

        let soli = project_solidarity(&base_solidarity(), year(2070), 44, &Assumptions::default())
            .expect("projects");
        assert_eq!(soli.rate, base_solidarity().rate);
        assert_eq!(soli.taper_rate, base_solidarity().taper_rate);
    }

    /// The ceilings must stay on their statutory grids at every horizon, or a projected
    /// figure would be visibly invented.
    #[test]
    fn projected_ceilings_stay_on_their_statutory_grids() {
        let base = base_social();
        for steps in 0_u32..=44 {
            let projected =
                project_social(&base, projected_year(steps), steps, &Assumptions::default())
                    .expect("projects");

            let pension_annual = projected
                .pension
                .ceiling_monthly
                .mul_int(12)
                .expect("in domain")
                .whole_euro_floor()
                .expect("in domain");
            assert_eq!(
                pension_annual % PENSION_CEILING_GRID_EURO,
                0,
                "after {steps} steps the pension ceiling {pension_annual} is off the grid"
            );

            let health_annual = projected
                .health
                .ceiling_monthly
                .mul_int(12)
                .expect("in domain")
                .whole_euro_floor()
                .expect("in domain");
            assert_eq!(
                health_annual % HEALTH_CEILING_GRID_EURO,
                0,
                "after {steps} steps the health ceiling {health_annual} is off the grid"
            );
        }
    }

    /// The unemployment ceiling must keep tracking the pension one, since the enacted
    /// tables are tested for exactly that equality.
    #[test]
    fn the_unemployment_ceiling_keeps_tracking_the_pension_ceiling() {
        let base = base_social();
        for steps in [0_u32, 1, 10, 40] {
            let projected =
                project_social(&base, projected_year(steps), steps, &Assumptions::default())
                    .expect("projects");
            assert_eq!(
                projected.unemployment.ceiling_monthly, projected.pension.ceiling_monthly,
                "the two ceilings diverged after {steps} steps"
            );
        }
    }

    /// The Rentenwert's mid-year continuity must survive projection: the value in force
    /// on 31 December must still be in force on 1 January.
    #[test]
    fn the_pension_value_stays_continuous_across_projected_years() {
        let base = base_social();
        let assumptions = Assumptions::default();
        for steps in 1_u32..=40 {
            let earlier =
                project_social(&base, projected_year(steps), steps, &assumptions).expect("a");
            let later = project_social(
                &base,
                projected_year(steps.saturating_add(1)),
                steps.saturating_add(1),
                &assumptions,
            )
            .expect("b");
            assert_eq!(
                earlier.pension.pension_value_jul_to_dec, later.pension.pension_value_jan_to_jun,
                "continuity broke between step {steps} and the next"
            );
        }
    }

    /// The joint Soli Freigrenze must remain exactly twice the individual one, which the
    /// enacted tables are also tested for.
    #[test]
    fn the_joint_soli_exemption_stays_exactly_double() {
        let base = base_solidarity();
        for steps in [0_u32, 1, 20, 44] {
            let projected =
                project_solidarity(&base, projected_year(steps), steps, &Assumptions::default())
                    .expect("projects");
            assert_eq!(
                projected.exemption_joint,
                projected
                    .exemption_individual
                    .mul_int(2)
                    .expect("in domain")
            );
        }
    }

    /// Wage-indexed figures grow, and grow monotonically.
    #[test]
    fn wage_indexed_figures_grow_monotonically() {
        let base = base_social();
        let mut previous = base.pension.average_earnings_annual;
        for steps in 1_u32..=40 {
            let projected =
                project_social(&base, projected_year(steps), steps, &Assumptions::default())
                    .expect("projects");
            let value = projected.pension.average_earnings_annual;
            assert!(value >= previous, "fell at step {steps}");
            previous = value;
        }
        assert!(previous > base.pension.average_earnings_annual);
    }

    /// Everything a projection emits reports itself as projected, including the parts
    /// carried forward unchanged. Unchanged figures are still assumptions.
    #[test]
    fn every_projected_part_reports_itself_as_projected() {
        let assumptions = Assumptions::default();
        let social = project_social(&base_social(), year(2040), 14, &assumptions).expect("a");
        assert!(!social.status().is_binding_law());

        let soli = project_solidarity(&base_solidarity(), year(2040), 14, &assumptions).expect("b");
        assert!(!soli.provenance.status.is_binding_law());

        let church = carry_forward_church_tax(
            &ChurchTaxParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted"),
            year(2040),
        );
        assert!(!church.provenance.status.is_binding_law());
        // The rates themselves are unchanged; only the claim about them is weaker.
        let base_church = ChurchTaxParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        assert_eq!(church.reduced_rate, base_church.reduced_rate);
        assert_eq!(church.standard_rate, base_church.standard_rate);

        let retirement = carry_forward_retirement(
            &RetirementParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted"),
            year(2040),
        );
        assert!(!retirement.provenance.status.is_binding_law());
    }

    /// Every projected provenance must say it is not enacted law, in words, because a
    /// reader may see the `legal_basis` string without the `DataStatus` beside it.
    #[test]
    fn every_projected_basis_says_it_is_not_law() {
        let assumptions = Assumptions::default();
        let social = project_social(&base_social(), year(2040), 14, &assumptions).expect("a");
        let soli = project_solidarity(&base_solidarity(), year(2040), 14, &assumptions).expect("b");
        for basis in [
            social.pension.provenance.legal_basis,
            soli.provenance.legal_basis,
        ] {
            assert!(
                basis.contains("NOT enacted law"),
                "the basis must say so in words: {basis}"
            );
        }
    }
}
