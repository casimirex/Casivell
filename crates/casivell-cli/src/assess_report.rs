//! Rendering the annual assessment: the § 2 EStG chain, stage by stage.
//!
//! Every intermediate is printed rather than only the answer, because the use this report is
//! for is reconciling a Steuerbescheid — and that means finding *which* line diverged. A
//! single output figure makes that impossible.

use core::fmt::Write as _;

use casivell_core::{Money, MoneyError};
use casivell_income::{Assessment, CapitalIncomeTax, CapitalRoute, ChildRelief, TaxableIncome};

use crate::format::euro;

/// Everything the report shows.
pub(crate) struct AssessmentReport {
    /// The year assessed.
    pub(crate) year: u16,
    /// The § 2 chain.
    pub(crate) income: TaxableIncome,
    /// The assessment built on it.
    pub(crate) assessment: Assessment,
    /// Capital income, where there was any.
    pub(crate) capital: Option<CapitalIncomeTax>,
    /// Whether § 32b raised the rate.
    pub(crate) benefits: Money,
}

/// Renders the assessment.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(report: &AssessmentReport) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(4_096);
    let _ = writeln!(out, "\nCasivell — Jahresveranlagung {}", report.year);
    let _ = writeln!(out, "  Estimated, not a Steuerbescheid. See the notes.\n");

    write_chain(&mut out, &report.income)?;
    write_assessment(&mut out, report)?;
    if let Some(capital) = report.capital {
        write_capital(&mut out, &capital)?;
    }
    write_notes(&mut out, report);
    Ok(out)
}

/// One labelled amount, with an optional note in the margin.
fn line(out: &mut String, label: &str, amount: Money, note: &str) -> Result<(), MoneyError> {
    let _ = writeln!(out, "    {label:<44} {:>12}  {note}", euro(amount)?);
    Ok(())
}

/// The § 2 chain from gross pay to the Einkommen.
fn write_chain(out: &mut String, income: &TaxableIncome) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Zu versteuerndes Einkommen (§ 2 EStG)");
    line(out, "Bruttoarbeitslohn", income.gross, "")?;
    line(
        out,
        "− Werbungskosten (§ 9, § 9a)",
        income.work_expenses_deducted,
        if income.work_expenses_lump_sum_used {
            "Pauschbetrag"
        } else {
            "actual"
        },
    )?;
    line(out, "= Einkünfte (§ 19)", income.employment_income, "")?;
    line(out, "= Gesamtbetrag der Einkünfte", income.total_income, "")?;

    let provision = &income.provision;
    line(
        out,
        "− Altersvorsorge (§ 10 Abs. 1 Nr. 2)",
        provision.retirement,
        if provision.retirement_cap_applied {
            "capped"
        } else {
            ""
        },
    )?;
    line(
        out,
        "− Sonstige Vorsorge (§ 10 Abs. 1 Nr. 3)",
        provision.other,
        if provision.other_cap_overridden {
            "Satz 4 override"
        } else {
            ""
        },
    )?;
    line(
        out,
        "− Übrige Sonderausgaben (§§ 10–10c)",
        income.other_special_expenses_deducted,
        if income.special_expenses_lump_sum_used {
            "Pauschbetrag"
        } else {
            "actual"
        },
    )?;
    line(out, "= Einkommen", income.income, "")?;
    let _ = writeln!(out);
    Ok(())
}

/// The tax, the child relief and the settlement.
fn write_assessment(out: &mut String, report: &AssessmentReport) -> Result<(), MoneyError> {
    let a = &report.assessment;
    let _ = writeln!(out, "  Steuer");
    line(
        out,
        "zu versteuerndes Einkommen",
        a.taxable_income,
        match a.child_relief {
            ChildRelief::Allowance { .. } => "after Kinderfreibetrag",
            _ => "",
        },
    )?;
    line(
        out,
        "Einkommensteuer (§ 32a)",
        a.income_tax,
        if report.benefits.is_zero() {
            ""
        } else {
            "at the § 32b rate"
        },
    )?;
    line(out, "Solidaritätszuschlag", a.solidarity_surcharge, "")?;
    line(out, "Kirchensteuer", a.church_tax, "")?;
    line(out, "= Gesamtbelastung", a.total_liability, "")?;
    let _ = writeln!(out);

    write_child_relief(out, a)?;
    write_settlement(out, a)?;
    Ok(())
}

/// The § 31 Günstigerprüfung, in words.
///
/// Which side won is the interesting part rather than the arithmetic: a household wants to
/// know whether claiming the allowance was worth anything, and for most incomes it is not.
fn write_child_relief(out: &mut String, a: &Assessment) -> Result<(), MoneyError> {
    match a.child_relief {
        ChildRelief::Allowance {
            deducted,
            clawed_back,
            advantage,
        } => {
            let _ = writeln!(out, "  Kinder (§ 31 Günstigerprüfung)");
            let _ = writeln!(
                out,
                "    The Kinderfreibetrag wins: {} € deducted, {} € Kindergeld added back,",
                euro(deducted)?,
                euro(clawed_back)?
            );
            let _ = writeln!(
                out,
                "    leaving the household {} € better off.\n",
                euro(advantage)?
            );
        }
        ChildRelief::ChildBenefit { received } => {
            let _ = writeln!(out, "  Kinder (§ 31 Günstigerprüfung)");
            let _ = writeln!(
                out,
                "    The Kindergeld of {} € wins; the Kinderfreibetrag is not granted.\n",
                euro(received)?
            );
        }
        ChildRelief::NotApplicable => {}
    }
    Ok(())
}

/// Withheld against owed, with the sign spelled out.
fn write_settlement(out: &mut String, a: &Assessment) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Abrechnung");
    line(out, "Lohnsteuer einbehalten", a.withheld, "")?;
    let owed = a.refund.is_negative();
    let _ = writeln!(
        out,
        "    {:<44} {:>12}  {}",
        if owed { "Nachzahlung" } else { "Erstattung" },
        euro(if owed { a.refund.neg()? } else { a.refund })?,
        if owed { "owed" } else { "back" }
    );
    let _ = writeln!(out);
    Ok(())
}

/// Capital income under § 32d, where there is any.
fn write_capital(out: &mut String, capital: &CapitalIncomeTax) -> Result<(), MoneyError> {
    let _ = writeln!(out, "  Kapitalerträge (§ 32d EStG)");
    let _ = writeln!(
        out,
        "    Gross {} €, less {} € Sparer-Pauschbetrag, leaves {} € taxable.",
        euro(capital.gross)?,
        euro(capital.allowance_applied)?,
        euro(capital.taxable)?
    );
    match capital.route {
        CapitalRoute::FlatRate => {
            let _ = writeln!(
                out,
                "    Taxed at the flat 25 % (Abs. 1); electing the tariff would cost more."
            );
        }
        CapitalRoute::OrdinaryTariff { saving } => {
            let _ = writeln!(
                out,
                "    The Abs. 6 election is worth {} € — apply for it on the return.",
                euro(saving)?
            );
        }
    }
    let _ = writeln!(
        out,
        "    Tax {} €, Soli {} €, Kirchensteuer {} €, total {} €.\n",
        euro(capital.income_tax)?,
        euro(capital.solidarity_surcharge)?,
        euro(capital.church_tax)?,
        euro(capital.total)?
    );
    Ok(())
}

/// The caveats. These are not decoration: this crate's assessment is explicitly inexact.
fn write_notes(out: &mut String, report: &AssessmentReport) {
    let _ = writeln!(out, "  Notes");
    let _ = writeln!(
        out,
        "  · An estimate, not a liability. § 10's interaction has not been reconciled"
    );
    let _ = writeln!(
        out,
        "    against a real Steuerbescheid, and außergewöhnliche Belastungen and the"
    );
    let _ = writeln!(out, "    other income categories are not modelled.");
    if !report.benefits.is_zero() {
        let _ = writeln!(
            out,
            "  · § 32b: the benefits are untaxed but raise the rate on everything else."
        );
    }
    let _ = writeln!(
        out,
        "  · Withheld is this salary's own Lohnsteuer for a full year at a steady rate."
    );
    let _ = writeln!(
        out,
        "    A year that was not steady will have withheld something different."
    );
    if !report.assessment.is_exact {
        let _ = writeln!(out, "  · The engine reports this assessment as inexact.");
    }
    let _ = writeln!(out, "\n  Not tax advice (§§ 1–4 StBerG).\n");
}
