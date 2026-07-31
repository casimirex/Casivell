//! Rendering the tax-class comparison.
//!
//! The report leads with the fact the reader most likely does not believe — that the annual
//! tax is the same under all three arrangements — because every figure below it only makes
//! sense once that is settled.

use core::fmt::Write as _;

use casivell_core::{Money, MoneyError};
use casivell_payroll::{ClassComparison, factor_thousandths};

use crate::format::euro;

/// Renders the comparison.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(
    comparison: &ClassComparison,
    higher: Money,
    lower: Money,
    year: u16,
) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(2_048);

    let _ = writeln!(out, "\nCasivell — Steuerklassenvergleich {year}");
    let _ = writeln!(
        out,
        "  Two salaries: {} € and {} € a month\n",
        euro(higher)?,
        euro(lower)?
    );

    let _ = writeln!(
        out,
        "  The annual income tax is {} € under all three. The tax class decides",
        euro(comparison.joint_liability)?
    );
    let _ = writeln!(
        out,
        "  when it is paid and by which spouse — not how much.\n"
    );

    let _ = writeln!(
        out,
        "  Arrangement          Higher/mo   Lower/mo     Net/mo     At assessment"
    );
    let _ = writeln!(out, "  {}", "─".repeat(70));

    let mut row =
        |name: &str, arrangement: &casivell_payroll::Arrangement| -> Result<(), MoneyError> {
            let _ = writeln!(
                out,
                "  {name:<18}  {:>9}  {:>9}  {:>9}  {:>16}",
                euro(arrangement.higher_withholding)?,
                euro(arrangement.lower_withholding)?,
                euro(arrangement.monthly_net)?,
                settlement(arrangement.settlement)?,
            );
            Ok(())
        };

    row("IV / IV", &comparison.four_four)?;
    row("III / V", &comparison.three_five)?;
    match comparison.factor {
        Some(factor) => {
            let mut label = String::with_capacity(24);
            let _ = write!(
                label,
                "IV + Faktor {}",
                format_factor(factor_thousandths(factor))
            );
            row(&label, &comparison.four_with_factor)?;
        }
        None => {
            let _ = writeln!(
                out,
                "  IV + Faktor         — not available: the factor is not below 1 (§ 39f Abs. 1)"
            );
        }
    }

    write_notes(&mut out);

    Ok(out)
}

/// The caveats the table needs to be read correctly.
fn write_notes(out: &mut String) {
    let _ = writeln!(out, "\n  Notes");
    let _ = writeln!(
        out,
        "  · A negative figure at assessment is a demand; a positive one a refund."
    );
    let _ = writeln!(
        out,
        "  · III/V takes least each month and owes most at the end. IV/IV is the"
    );
    let _ = writeln!(
        out,
        "    reverse. IV+Faktor aims to land near zero, and splits the burden"
    );
    let _ = writeln!(
        out,
        "    between the spouses in proportion to their salaries."
    );
    let _ = writeln!(
        out,
        "  · The choice does change wage-replacement benefits, which are computed"
    );
    let _ = writeln!(
        out,
        "    from net pay: Elterngeld, Arbeitslosengeld and Krankengeld are all"
    );
    let _ = writeln!(
        out,
        "    higher in class III than in class V, for the same household."
    );
    let _ = writeln!(
        out,
        "  · Settlement compares withheld income tax against the joint income tax."
    );
    let _ = writeln!(
        out,
        "    Solidaritätszuschlag and church tax follow it in the same direction."
    );
    let _ = writeln!(
        out,
        "  · Assumes both salaries run all year and no other income. The Faktor is"
    );
    let _ = writeln!(
        out,
        "    checked against § 32a rather than a Prüftabelle; none is published."
    );
    let _ = writeln!(out, "\n  Not tax advice (§§ 1–4 StBerG).\n");
}

/// A settlement with its sign spelled out, since a bare minus is easy to misread.
fn settlement(amount: Money) -> Result<String, MoneyError> {
    if amount.is_negative() {
        let owed = amount.neg()?;
        return Ok(format!("{} € owed", euro(owed)?));
    }
    Ok(format!("{} € back", euro(amount)?))
}

/// `0,873` from `873`.
fn format_factor(thousandths: i64) -> String {
    format!("0,{thousandths:03}")
}
