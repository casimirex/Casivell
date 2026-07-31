//! Rendering the statutory parameters for a year, enacted or projected.
//!
//! Payroll withholding cannot be projected — the Programmablaufplan is an annual
//! administrative instrument, not a formula — so a projected year has no payslip to
//! show. What it does have is a parameter set, and being able to *look at* that is what
//! makes an assumption reviewable rather than buried.
//!
//! The report leads with whether the figures are law or forecast, because that is the
//! single most important thing about them.

use casivell_core::{Money, MoneyError};
use casivell_lawdata::{DataStatus, Fingerprinted as _, LawYear};
use casivell_projection::Assumptions;

use crate::format::{euro, percent};

/// Column at which amounts are right-aligned.
const AMOUNT_COLUMN: usize = 52;

/// Renders the parameter set for a year.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(law: &LawYear, assumptions: &Assumptions) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(2_048);
    write_banner(&mut out, law, assumptions);
    write_income_tax(&mut out, law)?;
    write_social(&mut out, law)?;
    write_surcharges(&mut out, law)?;
    write_basis(&mut out, law);
    Ok(out)
}

fn line(out: &mut String, label: &str, amount: Money) -> Result<(), MoneyError> {
    use core::fmt::Write as _;
    let text = euro(amount)?;
    let width = AMOUNT_COLUMN
        .saturating_sub(label.chars().count())
        .max(text.chars().count());
    let _ = writeln!(out, "  {label}{text:>width$} €");
    Ok(())
}

fn rate_line(out: &mut String, label: &str, rate: casivell_core::Rate) -> Result<(), MoneyError> {
    use core::fmt::Write as _;
    let text = percent(rate)?;
    let width = AMOUNT_COLUMN
        .saturating_sub(label.chars().count())
        .max(text.chars().count());
    let _ = writeln!(out, "  {label}{text:>width$}");
    Ok(())
}

/// Leads with the status, because every figure below depends on it.
fn write_banner(out: &mut String, law: &LawYear, assumptions: &Assumptions) {
    use core::fmt::Write as _;
    let _ = writeln!(out, "\nCasivell — Rechengrößen {}", law.year.get());

    match law.status() {
        DataStatus::Enacted => {
            let _ = writeln!(out, "  ENACTED LAW — transcribed from primary sources.\n");
        }
        DataStatus::Draft => {
            let _ = writeln!(out, "  DRAFT — passed one chamber but not yet in force.\n");
        }
        DataStatus::Projected => {
            let _ = writeln!(
                out,
                "  ⚠  PROJECTED — NOT ENACTED LAW. No statute exists for {}.",
                law.year.get()
            );
            let prices = percent(assumptions.price_inflation()).unwrap_or_default();
            let wages = percent(assumptions.wage_growth()).unwrap_or_default();
            let _ = writeln!(
                out,
                "     Extrapolated from 2026 at {prices} price inflation and {wages} wage growth."
            );
            let _ = writeln!(
                out,
                "     Rates are held constant; the 45 % threshold is not indexed.\n"
            );
        }
    }
}

fn write_income_tax(out: &mut String, law: &LawYear) -> Result<(), MoneyError> {
    use core::fmt::Write as _;
    let t = &law.income_tax;
    let _ = writeln!(out, "  Einkommensteuertarif (§ 32a EStG)");
    line(
        out,
        "  Grundfreibetrag",
        Money::from_euro(t.basic_allowance_euro)?,
    )?;
    line(
        out,
        "  Ende Zone 2",
        Money::from_euro(t.first_progression.upper_bound_euro)?,
    )?;
    line(
        out,
        "  Beginn 42 % (Spitzensteuersatz)",
        Money::from_euro(t.upper_proportional.lower_bound_euro)?,
    )?;
    line(
        out,
        "  Beginn 45 % (Reichensteuer)",
        Money::from_euro(t.top_proportional.lower_bound_euro)?,
    )?;
    let _ = writeln!(out);
    Ok(())
}

fn write_social(out: &mut String, law: &LawYear) -> Result<(), MoneyError> {
    use core::fmt::Write as _;
    let s = &law.social;
    let _ = writeln!(out, "  Sozialversicherung");
    line(
        out,
        "  BBG Renten-/Arbeitslosenvers. (Monat)",
        s.pension.ceiling_monthly,
    )?;
    line(
        out,
        "  BBG Kranken-/Pflegevers. (Monat)",
        s.health.ceiling_monthly,
    )?;
    line(
        out,
        "  Durchschnittsentgelt (Jahr)",
        s.pension.average_earnings_annual,
    )?;
    line(out, "  Bezugsgröße (Monat)", s.reference_value_monthly)?;
    line(
        out,
        "  Aktueller Rentenwert (ab Juli)",
        s.pension.pension_value_jul_to_dec,
    )?;
    rate_line(
        out,
        "  Beitragssatz Rentenvers.",
        s.pension.contribution_rate,
    )?;
    rate_line(out, "  Beitragssatz Krankenvers.", s.health.general_rate)?;
    rate_line(out, "  Beitragssatz Pflegevers.", s.care.base_rate)?;
    let _ = writeln!(out);
    Ok(())
}

fn write_surcharges(out: &mut String, law: &LawYear) -> Result<(), MoneyError> {
    use core::fmt::Write as _;
    let _ = writeln!(out, "  Zuschläge");
    line(
        out,
        "  SolZ-Freigrenze (Einzelveranlagung)",
        law.solidarity.exemption_individual,
    )?;
    rate_line(out, "  SolZ-Satz", law.solidarity.rate)?;
    rate_line(out, "  Kirchensteuer (BW, BY)", law.church_tax.reduced_rate)?;
    rate_line(
        out,
        "  Kirchensteuer (übrige)",
        law.church_tax.standard_rate,
    )?;
    let _ = writeln!(out);
    Ok(())
}

fn write_basis(out: &mut String, law: &LawYear) {
    use core::fmt::Write as _;
    let _ = writeln!(out, "  Rechtsgrundlage");
    let _ = writeln!(out, "  · {}", law.income_tax.provenance.legal_basis);
    let _ = writeln!(out, "  · {}", law.social.pension.provenance.legal_basis);
    let _ = writeln!(
        out,
        "  · verified {}",
        law.income_tax.provenance.verified_on
    );
    // The digest of every figure above. Two runs quoting the same one computed against the
    // same law; a different one means a table moved, and the household is entitled to know
    // that rather than to wonder why a projection shifted.
    let _ = writeln!(out, "  · Datenstand {}\n", law.fingerprint());
    if !law.status().is_binding_law() {
        let _ = writeln!(
            out,
            "  These figures are an assumption, not a citation. Change the assumptions with"
        );
        let _ = writeln!(out, "  --inflation and --wage-growth.\n");
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use casivell_core::TaxYear;
    use casivell_lawdata::LawYear;
    use casivell_projection::{Assumptions, resolve};

    fn report(year_value: u16) -> String {
        let year = TaxYear::new(year_value).expect("representable");
        let assumptions = Assumptions::default();
        let law = resolve(year, &assumptions).expect("resolves");
        render(&law, &assumptions).expect("renders")
    }

    /// An enacted year must say so, and must not carry a projection warning.
    #[test]
    fn an_enacted_year_is_labelled_as_law() {
        let text = report(2026);
        assert!(text.contains("ENACTED LAW"));
        assert!(!text.contains("PROJECTED"));
        assert!(!text.contains("assumption, not a citation"));
        assert!(
            text.contains("12.348,00 €"),
            "the Grundfreibetrag is missing"
        );
    }

    /// A projected year must lead with the warning, name the assumptions, and say the
    /// figures are not a citation. This is the whole point of the view.
    #[test]
    fn a_projected_year_leads_with_the_warning_and_its_assumptions() {
        let text = report(2040);
        assert!(text.contains("PROJECTED"));
        assert!(text.contains("NOT ENACTED LAW"));
        assert!(text.contains("2,00 %"), "the price assumption is missing");
        assert!(text.contains("2,80 %"), "the wage assumption is missing");
        assert!(text.contains("assumption, not a citation"));
        assert!(!text.contains("ENACTED LAW — transcribed"));
    }

    /// Projected figures must actually differ from the enacted base, or the projection
    /// is doing nothing.
    #[test]
    fn projected_figures_differ_from_the_enacted_base() {
        let enacted = LawYear::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let assumptions = Assumptions::default();
        let projected = resolve(TaxYear::new(2046).unwrap(), &assumptions).unwrap();

        assert!(
            projected.income_tax.basic_allowance_euro > enacted.income_tax.basic_allowance_euro
        );
        assert!(projected.social.pension.ceiling_monthly > enacted.social.pension.ceiling_monthly);
        // But the unindexed 45 % threshold must not have moved.
        assert_eq!(
            projected.income_tax.top_proportional.lower_bound_euro,
            enacted.income_tax.top_proportional.lower_bound_euro
        );
    }

    /// Frozen assumptions must still be labelled projected — unchanged figures for a year
    /// with no statute are still an assumption.
    #[test]
    fn frozen_assumptions_are_still_labelled_projected() {
        let year = TaxYear::new(2040).expect("representable");
        let frozen = Assumptions::frozen();
        let law = resolve(year, &frozen).expect("resolves");
        let text = render(&law, &frozen).expect("renders");
        assert!(text.contains("PROJECTED"));
        assert!(text.contains("0,00 %"));
    }
}
