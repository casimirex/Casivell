//! Rendering a payslip, with the reasoning behind it.
//!
//! The point of the "how this was determined" section is not decoration. Casivell's
//! third commitment is that every figure traces back to the rule that produced it,
//! and the Vorsorgepauschale is the figure nobody can reproduce by hand — so the
//! report shows the chain from gross pay to taxable amount to tax, naming the PAP
//! variable at each step. A user reconciling against a real payslip can see exactly
//! where a divergence starts.
//!
//! It also states what is *not* modelled. A payslip that silently omits a
//! thirteenth month is worse than one that says it does not handle them.

use std::fmt::Write as _;

use casivell_core::{Money, MoneyError};
use casivell_lawdata::TaxClass;
use casivell_payroll::{NetPay, PayPeriod, PayrollLaw};

use crate::args::{Request, land_code};
use crate::format::{euro, percent, share_of};

/// Column at which amounts are right-aligned.
const AMOUNT_COLUMN: usize = 58;

/// Renders the full report.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(
    request: &Request,
    pay: &NetPay,
    law: &PayrollLaw,
) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(2_048);
    write_header(&mut out, request, pay)?;
    write_taxes(&mut out, request, pay)?;
    write_contributions(&mut out, pay)?;
    write_net(&mut out, pay)?;
    write_employer(&mut out, pay)?;
    write_derivation(&mut out, pay, law)?;
    write_caveats(&mut out, request, law)?;
    Ok(out)
}

/// Writes a label and a right-aligned amount.
fn line(out: &mut String, label: &str, amount: Money) -> Result<(), MoneyError> {
    let text = euro(amount)?;
    let width = AMOUNT_COLUMN
        .saturating_sub(label.chars().count())
        .max(text.chars().count());
    let _ = writeln!(out, "  {label}{text:>width$} €");
    Ok(())
}

/// Writes a label, a middle annotation, and a right-aligned amount.
fn line_with(out: &mut String, label: &str, note: &str, amount: Money) -> Result<(), MoneyError> {
    let text = euro(amount)?;
    let left = format!("{label:<30}{note:<10}");
    let width = AMOUNT_COLUMN
        .saturating_sub(left.chars().count())
        .max(text.chars().count());
    let _ = writeln!(out, "  {left}{text:>width$} €");
    Ok(())
}

fn rule(out: &mut String) {
    let _ = writeln!(out, "  {}", "─".repeat(AMOUNT_COLUMN.saturating_add(2)));
}

fn class_numeral(class: TaxClass) -> &'static str {
    match class {
        TaxClass::Class1 => "I",
        TaxClass::Class2 => "II",
        TaxClass::Class3 => "III",
        TaxClass::Class4 => "IV",
        TaxClass::Class5 => "V",
        TaxClass::Class6 => "VI",
    }
}

fn write_header(out: &mut String, request: &Request, pay: &NetPay) -> Result<(), MoneyError> {
    let period = match request.period {
        PayPeriod::Month => "monthly",
        PayPeriod::Year => "annual",
    };
    let _ = writeln!(out, "\nCasivell — Lohnabrechnung");
    let _ = writeln!(
        out,
        "  {} · Steuerklasse {} · {} · {period}",
        request.year,
        class_numeral(request.tax_class),
        land_code(request.land),
    );
    let _ = writeln!(out);
    line(out, "Bruttoentgelt", pay.gross)?;
    let _ = writeln!(out);
    Ok(())
}

fn write_taxes(out: &mut String, request: &Request, pay: &NetPay) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Steuern");
    line(out, "  Lohnsteuer", pay.income_tax.neg()?)?;
    line(
        out,
        "  Solidaritätszuschlag",
        pay.solidarity_surcharge.neg()?,
    )?;
    if request.church {
        line(out, "  Kirchensteuer", pay.church_tax.neg()?)?;
    }
    let _ = writeln!(out);
    Ok(())
}

fn write_contributions(out: &mut String, pay: &NetPay) -> Result<(), MoneyError> {
    // `monthly_contributions` is always a monthly breakdown; scale it to the period
    // being reported so the branch lines sum to the total shown above.
    let scale = pay.period.months();
    let c = &pay.monthly_contributions;
    let _ = writeln!(out, "  Sozialversicherung (Arbeitnehmeranteil)");
    let branches = [
        ("  Rentenversicherung", c.pension.employee),
        ("  Arbeitslosenversicherung", c.unemployment.employee),
        ("  Krankenversicherung", c.health.employee),
        ("  Pflegeversicherung", c.care.employee),
    ];
    for (label, monthly) in branches {
        let amount = monthly.mul_int(scale)?;
        let note = share_of(amount, pay.gross)?.unwrap_or_else(|| "—".to_owned());
        line_with(out, label, &note, amount.neg()?)?;
    }
    line(out, "  Summe", pay.employee_contributions.neg()?)?;
    let _ = writeln!(out);
    Ok(())
}

fn write_net(out: &mut String, pay: &NetPay) -> Result<(), MoneyError> {
    rule(out);
    line(out, "Nettoentgelt", pay.net)?;
    let share = share_of(pay.net, pay.gross)?.unwrap_or_else(|| "—".to_owned());
    let _ = writeln!(out, "  ({share} of gross)\n");
    Ok(())
}

fn write_employer(out: &mut String, pay: &NetPay) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Arbeitgeber");
    line(out, "  Sozialversicherung", pay.employer_contributions)?;
    line(out, "  Gesamtkosten der Beschäftigung", pay.employer_cost)?;
    let _ = writeln!(out);
    Ok(())
}

/// The explainability trace: how the Lohnsteuer figure was arrived at.
fn write_derivation(out: &mut String, pay: &NetPay, law: &PayrollLaw) -> Result<(), MoneyError> {
    let w = &pay.withholding;
    let _ = writeln!(
        out,
        "  Wie die Lohnsteuer ermittelt wurde (§ 39b EStG, BMF-PAP {})",
        law.year.get()
    );

    let annualised = pay.period.annualise(pay.gross)?;
    line(out, "  Jahresarbeitslohn (ZRE4)", annualised)?;
    line(
        out,
        "  − Tabellenfreibeträge (ZTABFB)",
        w.table_allowances.neg()?,
    )?;
    line(
        out,
        "  − Vorsorgepauschale (VSP)",
        w.vorsorgepauschale.neg()?,
    )?;
    line(
        out,
        "  = zu versteuernder Betrag (ZVE)",
        w.taxable_annual_amount,
    )?;
    line(out, "  Jahreslohnsteuer (LSTJAHR)", w.annual_income_tax)?;
    if pay.period == PayPeriod::Month {
        line(out, "  ÷ 12 = Lohnsteuer im Monat", w.income_tax)?;
    }

    if w.annual_church_tax_base != w.annual_income_tax {
        let _ = writeln!(out);
        line(
            out,
            "  Bemessungsgrundlage § 51a (JBMG)",
            w.annual_church_tax_base,
        )?;
        let _ = writeln!(
            out,
            "  (lower than the Lohnsteuer: § 51a Abs. 2 EStG recomputes it with the"
        );
        let _ = writeln!(
            out,
            "   full Kinderfreibetrag, which reduces church tax and Soli)"
        );
    }
    let _ = writeln!(out);
    Ok(())
}

/// States what the figures assume and what is not modelled.
fn write_caveats(out: &mut String, request: &Request, law: &PayrollLaw) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Assumptions and limits");
    let _ = writeln!(
        out,
        "  · Zusatzbeitrag {} — a published average, not your fund's rate. Pass --kvz.",
        percent(request.supplementary_rate)?
    );
    if request.children == 0 && !request.is_parent && request.age >= 23 {
        let _ = writeln!(
            out,
            "  · Childless surcharge applied (§ 55 Abs. 3 SGB XI). Pass --parent if wrong."
        );
    }
    let _ = writeln!(
        out,
        "  · Not modelled: one-off payments (§ 39b Abs. 3), Versorgungsbezüge,"
    );
    let _ = writeln!(
        out,
        "    Altersentlastungsbetrag, Faktorverfahren, ELStAM-Freibeträge."
    );
    let _ = writeln!(
        out,
        "  · Withholding is provisional. The annual assessment settles the difference."
    );
    let status = if law.payroll.provenance.status.is_binding_law() {
        "enacted law"
    } else {
        "NOT enacted law — a draft or projection"
    };
    let _ = writeln!(
        out,
        "  · Statutory basis: {status}, verified {}.",
        law.payroll.provenance.verified_on
    );
    let _ = writeln!(out, "\n  Not tax advice (§§ 1–4 StBerG).\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::args::Request;
    use casivell_core::TaxYear;
    use casivell_payroll::{Employment, HealthCover, PayrollLaw, monthly_net};
    use casivell_social::Insured;

    fn report_for(args: &[&str]) -> String {
        let request = Request::parse(args.iter().map(|s| (*s).to_owned())).expect("parses");
        let law = PayrollLaw::for_year(TaxYear::new(request.year).expect("year")).expect("law");
        let insured = Insured::new(
            request.age,
            request.is_parent,
            request.children,
            request.land,
            None,
        )
        .expect("profile");
        let employment = Employment::new(
            insured,
            request.tax_class,
            u16::from(request.children).saturating_mul(10),
            HealthCover::Statutory {
                supplementary_rate: request.supplementary_rate,
            },
            if request.church {
                Some(request.land)
            } else {
                None
            },
        )
        .expect("employment");
        let pay = monthly_net(request.gross, &employment, &law).expect("computes");
        render(&request, &pay, &law).expect("renders")
    }

    #[test]
    fn the_report_shows_the_headline_figures() {
        let text = report_for(&["--gross", "4500", "-c", "1"]);
        assert!(text.contains("Bruttoentgelt"));
        assert!(text.contains("4.500,00 €"));
        assert!(text.contains("Nettoentgelt"));
        assert!(text.contains("Lohnsteuer"));
    }

    /// The derivation chain is the explainability promise; it must always be present.
    #[test]
    fn the_report_shows_the_derivation_chain() {
        let text = report_for(&["--gross", "4500", "-c", "1"]);
        for marker in ["ZRE4", "ZTABFB", "VSP", "ZVE", "LSTJAHR"] {
            assert!(text.contains(marker), "the derivation omits {marker}");
        }
    }

    /// Church tax appears only for a member, and never as a stray zero line.
    #[test]
    fn church_tax_appears_only_when_levied() {
        assert!(!report_for(&["--gross", "4500", "-c", "1"]).contains("Kirchensteuer"));
        assert!(report_for(&["--gross", "4500", "-c", "1", "--church"]).contains("Kirchensteuer"));
    }

    /// With children the § 51a base diverges from the Lohnsteuer, and the report must
    /// explain the divergence rather than showing two unexplained numbers.
    #[test]
    fn the_report_explains_the_lower_church_tax_base_for_families() {
        let text = report_for(&["--gross", "6000", "-c", "1", "--children", "2", "--church"]);
        assert!(text.contains("JBMG"));
        assert!(text.contains("§ 51a Abs. 2"));
    }

    /// The childless-surcharge notice must appear exactly when the surcharge applies,
    /// since it is the assumption users are most likely to want to correct.
    #[test]
    fn the_childless_surcharge_notice_tracks_the_assumption() {
        assert!(report_for(&["--gross", "4500", "-c", "1"]).contains("Childless surcharge"));
        assert!(
            !report_for(&["--gross", "4500", "-c", "1", "--children", "1"])
                .contains("Childless surcharge")
        );
        assert!(
            !report_for(&["--gross", "4500", "-c", "1", "--age", "20"])
                .contains("Childless surcharge")
        );
    }

    /// Every report must state its limits and that it is not advice.
    #[test]
    fn the_report_always_states_its_limits() {
        let text = report_for(&["--gross", "4500", "-c", "1"]);
        assert!(text.contains("Not modelled"));
        assert!(text.contains("StBerG"));
        assert!(text.contains("provisional"));
        assert!(text.contains("Zusatzbeitrag"));
    }
}
