//! Projecting the Programmablaufplan's parameters.
//!
//! # A reversed decision, and why
//!
//! This crate originally declined to project [`PayrollParameters`], on the reasoning
//! that the Programmablaufplan is an annual administrative instrument rather than a
//! formula, and that a household projection wants the annual assessment rather than
//! payroll withholding.
//!
//! The first half of that is still true. The second half was wrong in a way that only
//! became visible once a simulation kernel needed a tax figure for 2055:
//!
//! - The annual assessment needs a *zu versteuerndes Einkommen*, and determining that —
//!   Werbungskosten, Sonderausgaben, the Vorsorgeaufwendungen limits — is not
//!   implemented. It is the largest remaining piece of the product.
//! - So the alternatives were to project the PAP's parameters, or to invent a
//!   simplified zvE model of our own.
//!
//! Withholding is the better of the two by a wide margin. It is not a rival to the
//! annual assessment; it is the statute's *own* approximation of it, designed so that
//! the two agree closely for an ordinary employee. It is a real algorithm rather than
//! our invention, and it is already verified against 516 official reference values. A
//! simplified zvE would have been a plausible-looking figure with nothing behind it —
//! precisely the failure `docs/ROADMAP_ERRATA.md` exists to prevent.
//!
//! What remains true is that the *structure* of the PAP is not projected: the
//! algorithm, its rounding points and its branches are taken as they stand in 2026.
//! Only the numbers move. A future reissue could restructure the Vorsorgepauschale, and
//! nothing here would anticipate that.
//!
//! # Treatment
//!
//! | Parameter | Treatment |
//! |---|---|
//! | Arbeitnehmer-Pauschbetrag, Sonderausgaben-Pauschbetrag, Entlastungsbetrag | price inflation |
//! | Kinderfreibetrag | price inflation, with the half kept exactly half |
//! | § 39b Abs. 2 Satz 7 thresholds | price inflation |
//! | Contribution ceilings | **taken from the projected social parameters**, not re-derived |
//! | Every rate | held constant |
//! | The 1 900 € Vorsorgepauschale cap | **held constant** |
//!
//! The ceilings are copied from the already-projected [`SocialParameters`] rather than
//! projected again. The enacted tables are tested for the two agreeing — the PAP's
//! annual figure equals twelve times the SVBezGrV's monthly one — and deriving them
//! twice would let a rounding difference break that. Copying makes the agreement
//! structural.
//!
//! The 1 900 € cap is held constant because it is a nominal amount set by legislation
//! and has not moved since the Vorsorgepauschale took its current shape. Holding it
//! constant is a substantive assumption, not a neutral one: as wages grow it binds on
//! more people, which is a real effect of leaving a nominal cap unindexed and one a
//! projection should show rather than smooth away.

use casivell_core::{Money, Rounding, TaxYear};
use casivell_lawdata::{IncomeTaxTariff, PayrollParameters, SocialParameters};

use crate::ProjectionError;
use crate::assumptions::Assumptions;
use crate::growth::compound_to_euro;
use crate::parameters::projected_provenance;

/// Months in a year, for converting the social parameters' monthly ceilings.
const MONTHS: i64 = 12;

/// Projects the Programmablaufplan's parameters.
///
/// `social` must be the *projected* social parameters for the same year, so the two
/// agree on the contribution ceilings. `tariff` is likewise the projected tariff, whose
/// marginal rates the class V/VI bands mirror.
///
/// # Errors
///
/// [`ProjectionError::Arithmetic`] on a domain violation.
pub fn project_payroll(
    base: &PayrollParameters,
    social: &SocialParameters,
    tariff: &IncomeTaxTariff,
    year: TaxYear,
    steps: u32,
    assumptions: &Assumptions,
) -> Result<PayrollParameters, ProjectionError> {
    let prices = assumptions.price_inflation();
    let grow = |amount: Money| -> Result<Money, ProjectionError> {
        compound_to_euro(amount, prices, steps).map_err(ProjectionError::Arithmetic)
    };

    // The Kinderfreibetrag's half must stay exactly half, since both parents in class IV
    // share one allowance and the enacted tables are tested for the relationship.
    let child_allowance_full = grow(base.child_allowance_full)?;
    let child_allowance_half = child_allowance_full
        .div_int(2, Rounding::Floor)
        .map_err(ProjectionError::Arithmetic)?;

    Ok(PayrollParameters {
        year,

        employee_lump_sum: grow(base.employee_lump_sum)?,
        special_expenses_lump_sum: grow(base.special_expenses_lump_sum)?,
        single_parent_relief: grow(base.single_parent_relief)?,
        child_allowance_full,
        child_allowance_half,

        // Rates are political decisions with no indexation rule; see the crate docs.
        vorsorge_pension_rate: base.vorsorge_pension_rate,
        vorsorge_unemployment_rate: base.vorsorge_unemployment_rate,
        vorsorge_health_half_rate: base.vorsorge_health_half_rate,
        vorsorge_care_rate: base.vorsorge_care_rate,
        vorsorge_care_rate_saxony: base.vorsorge_care_rate_saxony,
        vorsorge_care_childless_surcharge: base.vorsorge_care_childless_surcharge,
        vorsorge_care_child_reduction: base.vorsorge_care_child_reduction,
        vorsorge_care_max_reductions: base.vorsorge_care_max_reductions,
        // Nominal and unindexed. See the crate documentation on why that matters.
        vorsorge_unemployment_health_cap: base.vorsorge_unemployment_health_cap,

        // Copied from the projected social parameters so the two cannot disagree.
        ceiling_pension_unemployment_annual: annualise(social.pension.ceiling_monthly)?,
        ceiling_health_care_annual: annualise(social.health.ceiling_monthly)?,

        class_five_six_threshold_1: grow(base.class_five_six_threshold_1)?,
        class_five_six_threshold_2: grow(base.class_five_six_threshold_2)?,
        class_five_six_threshold_3: grow(base.class_five_six_threshold_3)?,
        // The class V/VI bands mirror the tariff's own marginal rates, so they are taken
        // from the projected tariff rather than carried forward independently.
        class_five_six_min_rate: base.class_five_six_min_rate,
        class_five_six_upper_rate: tariff.upper_proportional.marginal_rate,
        class_five_six_top_rate: tariff.top_proportional.marginal_rate,

        provenance: projected_provenance("PAP, projected", base.provenance.source_url, assumptions),
    })
}

/// Twelve times a monthly amount.
fn annualise(monthly: Money) -> Result<Money, ProjectionError> {
    monthly.mul_int(MONTHS).map_err(ProjectionError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::project_payroll;
    use crate::assumptions::Assumptions;
    use crate::{parameters, tariff};
    use casivell_core::TaxYear;
    use casivell_lawdata::{IncomeTaxTariff, PayrollParameters, SocialParameters, TaxClass};

    fn year(value: u16) -> TaxYear {
        TaxYear::new(value).expect("representable")
    }

    /// Projects the whole set for a horizon, so the ceilings genuinely come from the
    /// projected social parameters rather than a stale enacted copy.
    fn projected(steps: u32, assumptions: &Assumptions) -> PayrollParameters {
        let target = year(
            TaxYear::LAST_VERIFIED
                .get()
                .saturating_add(u16::try_from(steps).expect("within a century")),
        );
        let base_payroll = PayrollParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        let base_social = SocialParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        let base_tariff = IncomeTaxTariff::for_year(TaxYear::LAST_VERIFIED).expect("enacted");

        let social =
            parameters::project_social(&base_social, target, steps, assumptions).expect("social");
        let tax = tariff::project_tariff(&base_tariff, target, steps, assumptions).expect("tariff");
        project_payroll(&base_payroll, &social, &tax, target, steps, assumptions).expect("payroll")
    }

    /// Frozen assumptions must reproduce the enacted parameters exactly, so projection
    /// contributes nothing of its own when asked to change nothing.
    #[test]
    fn frozen_assumptions_reproduce_the_enacted_parameters() {
        let base = PayrollParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        let p = projected(24, &Assumptions::frozen());

        assert_eq!(p.employee_lump_sum, base.employee_lump_sum);
        assert_eq!(p.special_expenses_lump_sum, base.special_expenses_lump_sum);
        assert_eq!(p.single_parent_relief, base.single_parent_relief);
        assert_eq!(p.child_allowance_full, base.child_allowance_full);
        assert_eq!(p.child_allowance_half, base.child_allowance_half);
        assert_eq!(
            p.ceiling_pension_unemployment_annual,
            base.ceiling_pension_unemployment_annual
        );
        assert_eq!(
            p.ceiling_health_care_annual,
            base.ceiling_health_care_annual
        );
        assert_eq!(
            p.class_five_six_threshold_1,
            base.class_five_six_threshold_1
        );
        assert_eq!(
            p.class_five_six_threshold_2,
            base.class_five_six_threshold_2
        );
        assert_eq!(
            p.class_five_six_threshold_3,
            base.class_five_six_threshold_3
        );
    }

    /// The invariant the enacted tables are tested for must survive projection: the PAP's
    /// annual ceilings equal twelve times the social parameters' monthly ones.
    ///
    /// This is why the ceilings are copied rather than projected twice.
    #[test]
    fn the_projected_ceilings_agree_with_the_projected_social_parameters() {
        let assumptions = Assumptions::default();
        for steps in [0_u32, 1, 20, 44] {
            let target = year(
                TaxYear::LAST_VERIFIED
                    .get()
                    .saturating_add(u16::try_from(steps).expect("small")),
            );
            let base_social = SocialParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
            let social = parameters::project_social(&base_social, target, steps, &assumptions)
                .expect("social");
            let p = projected(steps, &assumptions);

            assert_eq!(
                p.ceiling_pension_unemployment_annual,
                social
                    .pension
                    .ceiling_monthly
                    .mul_int(12)
                    .expect("in domain"),
                "the pension ceilings disagreed after {steps} steps"
            );
            assert_eq!(
                p.ceiling_health_care_annual,
                social
                    .health
                    .ceiling_monthly
                    .mul_int(12)
                    .expect("in domain"),
                "the health ceilings disagreed after {steps} steps"
            );
        }
    }

    /// The class IV Kinderfreibetrag must stay exactly half the full one at every
    /// horizon, or the two parents' halves would not sum to one allowance.
    #[test]
    fn the_child_allowance_half_stays_exactly_half() {
        let assumptions = Assumptions::default();
        for steps in [0_u32, 1, 7, 20, 44] {
            let p = projected(steps, &assumptions);
            assert_eq!(
                p.child_allowance_half.mul_int(2).expect("in domain"),
                p.child_allowance_full,
                "the halves stopped summing after {steps} steps"
            );
        }
    }

    /// Allowances grow with prices and rates do not move.
    #[test]
    fn allowances_grow_while_rates_are_held() {
        let base = PayrollParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        let p = projected(20, &Assumptions::default());

        assert!(p.employee_lump_sum > base.employee_lump_sum);
        assert!(p.single_parent_relief > base.single_parent_relief);
        assert!(p.child_allowance_full > base.child_allowance_full);
        assert!(p.class_five_six_threshold_1 > base.class_five_six_threshold_1);

        assert_eq!(p.vorsorge_pension_rate, base.vorsorge_pension_rate);
        assert_eq!(p.vorsorge_health_half_rate, base.vorsorge_health_half_rate);
        assert_eq!(p.vorsorge_care_rate, base.vorsorge_care_rate);
        assert_eq!(p.class_five_six_min_rate, base.class_five_six_min_rate);
    }

    /// The 1 900 EUR Vorsorgepauschale cap is nominal and unindexed, so it must not move.
    /// That it then binds on more people over time is a real effect the projection should
    /// show, not a defect.
    #[test]
    fn the_vorsorgepauschale_cap_is_held_constant() {
        let base = PayrollParameters::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        for steps in [1_u32, 20, 44] {
            let p = projected(steps, &Assumptions::default());
            assert_eq!(
                p.vorsorge_unemployment_health_cap, base.vorsorge_unemployment_health_cap,
                "the cap moved after {steps} steps"
            );
        }
    }

    /// The class V/VI rates must track the projected tariff's marginal rates, since they
    /// mirror them by construction in the enacted tables.
    #[test]
    fn the_class_five_six_rates_track_the_projected_tariff() {
        let assumptions = Assumptions::default();
        let steps = 20_u32;
        let target = year(TaxYear::LAST_VERIFIED.get().saturating_add(20));
        let base_tariff = IncomeTaxTariff::for_year(TaxYear::LAST_VERIFIED).expect("enacted");
        let tax =
            tariff::project_tariff(&base_tariff, target, steps, &assumptions).expect("tariff");
        let p = projected(steps, &assumptions);

        assert_eq!(
            p.class_five_six_upper_rate,
            tax.upper_proportional.marginal_rate
        );
        assert_eq!(
            p.class_five_six_top_rate,
            tax.top_proportional.marginal_rate
        );
    }

    /// Class VI still gets no lump sums, and classes V and VI no Kinderfreibetrag, at any
    /// horizon. Projection must not accidentally grant an allowance that does not exist.
    #[test]
    fn the_class_exclusions_survive_projection() {
        let p = projected(30, &Assumptions::default());
        assert!(p.employee_allowance_for(TaxClass::Class6).is_zero());
        assert!(p.special_expenses_allowance_for(TaxClass::Class6).is_zero());
        assert!(p.child_allowance_for(TaxClass::Class5).is_zero());
        assert!(p.child_allowance_for(TaxClass::Class6).is_zero());
        // And the classes that do get them still do.
        assert!(!p.employee_allowance_for(TaxClass::Class1).is_zero());
        assert!(!p.child_allowance_for(TaxClass::Class1).is_zero());
    }

    #[test]
    fn a_projected_payroll_set_never_claims_to_be_law() {
        let p = projected(10, &Assumptions::default());
        assert!(!p.provenance.status.is_binding_law());
        assert!(p.provenance.legal_basis.contains("NOT enacted law"));
    }
}
