//! Comparing two saved projections.
//!
//! # Why summaries rather than a text diff
//!
//! The obvious thing is to diff the two rendered reports. That would be at the mercy of column
//! widths, would flag every year that moved by a cent, and would say nothing about *how much*
//! the scenarios differ — which is the only question anyone is asking.
//!
//! So both are re-run through the kernel and their [`Summary`] figures compared. The output is
//! the handful of numbers a decision actually turns on, each with its difference.
//!
//! # Two checks, and why one refuses and the other warns
//!
//! The scenarios must be **projections**, and that is a refusal: a payslip and a forty-year
//! plan share no figures, so there is nothing to render.
//!
//! They should also rest on the **same statutory data**, and that is a *warning*. Comparing a
//! plan computed under last year's tables against one computed under this year's attributes to
//! the household's choices a difference that is partly the law moving underneath — exactly the
//! confusion the fingerprint exists to prevent, and it would be perverse to record it and then
//! say nothing. But the comparison is still worth having, and re-saving both files fixes it,
//! so the warning goes at the top and the figures follow.

use core::fmt::Write as _;

use casivell_core::{Money, MoneyError};
use casivell_sim::Summary;

use crate::format::euro;
use crate::scenario::Scenario;

/// One side of the comparison: where it came from, what it said, and what it produced.
type Side<'a> = (&'a String, &'a Scenario, &'a Summary);

/// Renders the comparison.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(left: Side<'_>, right: Side<'_>) -> Result<String, MoneyError> {
    let (left_path, left_saved, left_summary) = left;
    let (right_path, right_saved, right_summary) = right;

    let mut out = String::with_capacity(2_048);
    let _ = writeln!(out, "\nCasivell — Szenarienvergleich");
    let _ = writeln!(out, "  A  {left_path}");
    let _ = writeln!(out, "  B  {right_path}\n");

    if left_saved.fingerprint != right_saved.fingerprint {
        let _ = writeln!(
            out,
            "  ⚠ These scenarios rest on different statutory data ({} against {}).",
            left_saved.fingerprint, right_saved.fingerprint
        );
        let _ = writeln!(
            out,
            "    Part of any difference below is the law moving, not the household's"
        );
        let _ = writeln!(out, "    choices. Re-save both before comparing.\n");
    }

    write_arguments(&mut out, left_saved, right_saved);
    write_outcomes(&mut out, left_summary, right_summary)?;
    write_notes(&mut out);
    Ok(out)
}

/// What differs in the two invocations, which is usually the point of the comparison.
fn write_arguments(out: &mut String, left: &Scenario, right: &Scenario) {
    let only_in = |a: &Scenario, b: &Scenario| -> Vec<String> {
        a.args
            .iter()
            .filter(|arg| !b.args.contains(arg))
            .cloned()
            .collect()
    };
    let (left_only, right_only) = (only_in(left, right), only_in(right, left));

    let _ = writeln!(out, "  Was sich unterscheidet");
    if left_only.is_empty() && right_only.is_empty() {
        let _ = writeln!(out, "    Nothing — the two invocations are identical.\n");
        return;
    }
    if !left_only.is_empty() {
        let _ = writeln!(out, "    only A:  {}", left_only.join(" "));
    }
    if !right_only.is_empty() {
        let _ = writeln!(out, "    only B:  {}", right_only.join(" "));
    }
    let _ = writeln!(out);
}

/// The figures a decision turns on.
fn write_outcomes(out: &mut String, left: &Summary, right: &Summary) -> Result<(), MoneyError> {
    let _ = writeln!(
        out,
        "  Ergebnis nach {} Jahren        {:>14}  {:>14}  {:>14}",
        left.months / 12,
        "A",
        "B",
        "B − A"
    );
    let _ = writeln!(out, "  {}", "─".repeat(78));

    let mut row = |label: &str, a: Money, b: Money| -> Result<(), MoneyError> {
        let _ = writeln!(
            out,
            "  {label:<28}  {:>14}  {:>14}  {:>14}",
            euro(a)?,
            euro(b)?,
            euro(b.sub(a)?)?
        );
        Ok(())
    };

    row("Nettovermögen", left.final_net_worth, right.final_net_worth)?;
    row("Geldvermögen", left.final_wealth, right.final_wealth)?;
    row("Tiefster Stand", left.minimum_wealth, right.minimum_wealth)?;
    row("Steuern einbehalten", left.total_tax, right.total_tax)?;
    row(
        "Erstattungen",
        left.total_settlements,
        right.total_settlements,
    )?;
    row(
        "Sozialabgaben",
        left.total_contributions,
        right.total_contributions,
    )?;
    row(
        "Elterngeld",
        left.total_parental_benefit,
        right.total_parental_benefit,
    )?;
    row(
        "Hypothekenzinsen",
        left.total_mortgage_interest,
        right.total_mortgage_interest,
    )?;

    // Entgeltpunkte are not money, so they are shown apart rather than forced into the column.
    let _ = writeln!(
        out,
        "  {:<28}  {:>14}  {:>14}  {:>14}",
        "Entgeltpunkte",
        points(left.final_pension_points.micro()),
        points(right.final_pension_points.micro()),
        points(
            right
                .final_pension_points
                .micro()
                .saturating_sub(left.final_pension_points.micro())
        )
    );
    let _ = writeln!(out);
    Ok(())
}

/// Entgeltpunkte with two decimals, as the DRV reports them.
fn points(micro: i64) -> String {
    let hundredths = micro.saturating_add(5_000) / 10_000;
    let whole = hundredths / 100;
    let fraction = (hundredths % 100).unsigned_abs();
    format!("{whole},{fraction:02}")
}

/// What the comparison does and does not settle.
fn write_notes(out: &mut String) {
    let _ = writeln!(out, "  Notes");
    let _ = writeln!(
        out,
        "  · Nettovermögen includes any property, less its mortgage; Geldvermögen"
    );
    let _ = writeln!(
        out,
        "    does not. A buyer's cash is lower and their total may not be."
    );
    let _ = writeln!(
        out,
        "  · Tiefster Stand is the lowest financial wealth reached at any point. An"
    );
    let _ = writeln!(
        out,
        "    end state can hide a plan that passed through insolvency on the way."
    );
    let _ = writeln!(
        out,
        "  · Both scenarios use their own saved assumptions. Where those differ, the"
    );
    let _ = writeln!(
        out,
        "    difference below is partly the assumptions and not only the choices."
    );
    let _ = writeln!(out, "\n  Not tax or investment advice.\n");
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::scenario::Scenario;
    use casivell_sim::Summary;

    fn scenario(args: &[&str], fingerprint: &str) -> Scenario {
        let mut saved = Scenario::capture(
            Some("project".to_owned()),
            args.iter().map(|a| (*a).to_owned()).collect(),
            2026,
        )
        .expect("captures");
        saved.fingerprint = fingerprint.to_owned();
        saved
    }

    fn report(left: &Scenario, right: &Scenario) -> String {
        let (a, b) = (Summary::default(), Summary::default());
        let (left_path, right_path) = ("a.casivell".to_owned(), "b.casivell".to_owned());
        render((&left_path, left, &a), (&right_path, right, &b)).expect("renders")
    }

    /// The comparison must name what differs between the invocations, which is usually the
    /// question being asked.
    #[test]
    fn it_names_the_arguments_that_differ() {
        let text = report(
            &scenario(&["--gross", "6000"], "x"),
            &scenario(&["--gross", "6000", "--buy", "3:400000:100000:NW"], "x"),
        );
        assert!(text.contains("only B:"));
        assert!(text.contains("--buy"));
        assert!(!text.contains("only A:"), "nothing is unique to A here");
    }

    /// Identical invocations must say so rather than printing two empty lists.
    #[test]
    fn identical_invocations_are_reported_as_such() {
        let text = report(
            &scenario(&["--gross", "6000"], "x"),
            &scenario(&["--gross", "6000"], "x"),
        );
        assert!(text.contains("Nothing — the two invocations are identical."));
        assert!(!text.contains("only A:"));
        assert!(!text.contains("only B:"));
    }

    /// Different statutory data must be flagged, and flagged *before* the figures — a caveat
    /// under the numbers is a caveat nobody reads, and this one says part of the difference is
    /// not the household's doing.
    #[test]
    fn differing_statutory_data_is_flagged_above_the_figures() {
        let text = report(
            &scenario(&["--gross", "6000"], "1111111111111111"),
            &scenario(&["--gross", "6000"], "2222222222222222"),
        );
        let warning = text.find("different statutory data").expect("must warn");
        let figures = text.find("Ergebnis nach").expect("must have figures");
        assert!(warning < figures, "the warning must come first");
        assert!(text.contains("1111111111111111"));
        assert!(text.contains("not the household's"));
    }

    /// Matching data must not produce a warning, or the warning would be noise.
    #[test]
    fn matching_statutory_data_is_silent() {
        let text = report(
            &scenario(&["--gross", "6000"], "x"),
            &scenario(&["--gross", "7000"], "x"),
        );
        assert!(!text.contains("different statutory data"));
    }

    /// Every figure a decision turns on must appear, including the ones that are zero — a
    /// household comparing two plans wants to see that the tax did *not* change.
    #[test]
    fn every_compared_figure_is_labelled() {
        let text = report(
            &scenario(&["--gross", "6000"], "x"),
            &scenario(&["--gross", "6000"], "x"),
        );
        for label in [
            "Nettovermögen",
            "Geldvermögen",
            "Tiefster Stand",
            "Steuern einbehalten",
            "Erstattungen",
            "Sozialabgaben",
            "Elterngeld",
            "Hypothekenzinsen",
            "Entgeltpunkte",
        ] {
            assert!(text.contains(label), "the report should show {label}");
        }
    }
}
