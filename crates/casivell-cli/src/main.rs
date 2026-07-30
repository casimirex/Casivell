//! `casivell` — a command-line front end for the Casivell engine.
//!
//! # Why this exists
//!
//! Until now the engine had 208 passing tests and no way for a human to run it. That
//! is a real gap, and not only for demonstration: tests written by the author of the
//! code cannot catch a mistake in what the *inputs mean*. A person checking this
//! against their own payslip can, and that is a kind of verification no unit test
//! provides.
//!
//! # Structure
//!
//! This crate is the only one in the workspace that uses `std`. The engine stays
//! `#![no_std]`; formatting, argument parsing and I/O live here. That boundary is
//! deliberate — it is what keeps the guarantee that the calculation layer cannot
//! allocate or open a socket.
//!
//! Like the engine, it has no third-party dependencies.

// As in every other crate: panicking constructs are denied in shipped code and
// permitted only under `cfg(test)`, where a failed call on a hard-coded literal *is*
// the failure being reported. See docs/CODING_STANDARD.md R7.
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

mod args;
mod format;
mod report;

use std::process::ExitCode;

use casivell_core::TaxYear;
use casivell_payroll::{Employment, HealthCover, PayrollLaw, net_pay};
use casivell_social::Insured;

use args::Request;

/// Usage text, printed for `--help` and on any argument error.
const USAGE: &str = "\
casivell — German payroll and net pay, computed from statute

USAGE
  casivell --gross <amount> --class <1-6> [options]

REQUIRED
  -g, --gross <amount>    Gross pay for the period (4500, 4500,50 or 4.500,50)
  -c, --class <1-6>       Lohnsteuerklasse, as 1-6 or I-VI

OPTIONS
  -p, --period <p>        `month` (default) or `year`
  -y, --year <year>       Tax year (default 2026)
  -s, --state <code>      Two-letter state code, e.g. NW, BY, SN (default NW)
      --age <years>       Age in whole years (default 30)
      --children <n>      Children under 25, which reduce the care rate
      --parent            Has ever had a child, so no childless surcharge
      --church            Levy church tax
      --kvz <percent>     The health fund's full supplementary rate (default 2,9)
  -h, --help              This text

EXAMPLES
  casivell --gross 4500 --class 1
  casivell --gross 3200 --class 3 --state BY --children 2 --church
  casivell --gross 72000 --period year --class 1 --kvz 1,7

Weekly and daily pay periods are not supported: the PAP scales them by 360/7 and
1/360, which do not terminate in decimal, and approximating them would disagree
with payroll. See docs in casivell-payroll.

Not tax advice (§§ 1-4 StBerG).
";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match run(argv) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("casivell: {message}\n");
            eprintln!("Run `casivell --help` for usage.");
            ExitCode::FAILURE
        }
    }
}

/// Parses, computes and renders. Returns a message suitable for stderr on failure.
fn run(argv: Vec<String>) -> Result<String, String> {
    let request = Request::parse(argv).map_err(|e| e.to_string())?;

    let year = TaxYear::new(request.year).map_err(|_| {
        format!(
            "no verified statutory data for {}. Supported: {}–{}. \
             Casivell refuses to compute a year it cannot cite rather than \
             substituting a nearby one.",
            request.year,
            TaxYear::MIN.get(),
            TaxYear::MAX.get(),
        )
    })?;

    let law = PayrollLaw::for_year(year).map_err(|_| {
        format!(
            "the Programmablaufplan for {} has not been transcribed, so withholding \
             cannot be computed for that year. Only 2026 is available.",
            request.year,
        )
    })?;

    let employment = build_employment(&request).map_err(|e| e.to_string())?;
    let report = compute(&request, &employment, &law).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Assembles the engine's input types from the parsed request.
fn build_employment(request: &Request) -> Result<Employment, casivell_core::MoneyError> {
    let insured = Insured::new(
        request.age,
        request.is_parent,
        request.children,
        request.land,
        // The CLI passes the fund rate through the payroll path, where the PAP halves
        // it. Leaving this `None` keeps the contribution calculation on the published
        // average, which is the same figure unless the user overrides it.
        Some(request.supplementary_rate),
    )?;

    // The CLI ties Kinderfreibeträge to the number of children under 25. In reality
    // the two can diverge — a child over 18 in education still attracts a
    // Kinderfreibetrag while no longer reducing the care rate — so this is a
    // simplification, and the report's caveats say the figures are provisional.
    let allowance_tenths = u16::from(request.children).saturating_mul(10);

    Employment::new(
        insured,
        request.tax_class,
        allowance_tenths,
        HealthCover::Statutory {
            supplementary_rate: request.supplementary_rate,
        },
        if request.church {
            Some(request.land)
        } else {
            None
        },
    )
}

/// Runs the calculation for the requested period and renders it.
fn compute(
    request: &Request,
    employment: &Employment,
    law: &PayrollLaw,
) -> Result<String, casivell_core::MoneyError> {
    let pay = net_pay(request.gross, request.period, employment, law)?;
    report::render(request, &pay, law)
}

#[cfg(test)]
mod tests {
    use super::run;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_minimal_invocation_produces_a_report() {
        let text = run(argv(&["--gross", "4500", "--class", "1"])).expect("succeeds");
        assert!(text.contains("Nettoentgelt"));
    }

    /// An unsupported year must be refused with an explanation naming the range,
    /// never answered with a nearby year's figures.
    #[test]
    fn an_unsupported_year_is_refused_with_an_explanation() {
        let err = run(argv(&["--gross", "4500", "-c", "1", "-y", "2040"])).expect_err("refuses");
        assert!(err.contains("2040"), "the message omits the year: {err}");
        assert!(
            err.contains("refuses"),
            "the message omits the reason: {err}"
        );
    }

    /// 2025 has tariff and social data but no transcribed PAP, so withholding must be
    /// refused with a message that distinguishes the two situations.
    #[test]
    fn a_year_without_a_transcribed_pap_is_refused_distinctly() {
        let err = run(argv(&["--gross", "4500", "-c", "1", "-y", "2025"])).expect_err("refuses");
        assert!(
            err.contains("Programmablaufplan"),
            "the message should name the missing document: {err}"
        );
    }

    #[test]
    fn argument_errors_surface_as_messages_rather_than_panics() {
        assert!(run(argv(&["--gross", "abc", "-c", "1"])).is_err());
        assert!(run(argv(&["--class", "1"])).is_err());
        assert!(run(argv(&["--gross", "1", "-c", "1", "--bogus"])).is_err());
    }

    #[test]
    fn an_annual_request_renders() {
        let text = run(argv(&["--gross", "54000", "-c", "1", "-p", "year"])).expect("succeeds");
        assert!(text.contains("annual"));
    }

    /// End-to-end against the official BMF Prüftabelle. `casivell-payroll` already
    /// verifies all 516 values, but that tests the *engine*; this tests the wiring —
    /// that the CLI's defaults and flag semantics actually reach the engine as the
    /// table's stated parameters (ALV = KRV = PKV = 0, KVZ = 2,90, PVZ = 1).
    ///
    /// A mistake in what an input *means* is invisible to a unit test written by the
    /// author of the code it tests. This is the cheapest guard against that class.
    #[test]
    fn the_cli_reproduces_official_pruef_tabelle_values() {
        // (annual gross, tax class flag, expected annual Lohnsteuer as rendered)
        let cases = [
            (55_000, "1", "8.060,00"),
            (55_000, "2", "6.807,00"),
            (55_000, "3", "3.802,00"),
            (55_000, "4", "8.060,00"),
            (55_000, "5", "13.687,00"),
            (55_000, "6", "14.218,00"),
            (30_000, "1", "2.248,00"),
            (100_000, "1", "23.248,00"),
        ];
        for (gross, class, expected) in cases {
            let gross = gross.to_string();
            // Class II's table row assumes PVZ = 0, i.e. a parent.
            let mut args = vec![
                "--gross",
                gross.as_str(),
                "-c",
                class,
                "-p",
                "year",
                "--age",
                "40",
            ];
            if class == "2" {
                args.push("--children");
                args.push("1");
            }
            let text = run(argv(&args)).expect("succeeds");
            assert!(
                text.contains(expected),
                "class {class} at {gross} EUR should show {expected}; got:\n{text}"
            );
        }
    }

    /// The childless surcharge notice must not appear for a parent, since it is the
    /// assumption most likely to be silently wrong on a real payslip.
    #[test]
    fn the_report_reflects_the_parenthood_flags() {
        let childless = run(argv(&["--gross", "4500", "-c", "1"])).expect("succeeds");
        let parent = run(argv(&["--gross", "4500", "-c", "1", "--parent"])).expect("succeeds");
        assert!(childless.contains("Childless surcharge"));
        assert!(!parent.contains("Childless surcharge"));
        // And the parent keeps more, because the care rate is lower.
        assert_ne!(childless, parent);
    }
}
