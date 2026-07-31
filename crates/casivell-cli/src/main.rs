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
mod assess_report;
mod classes_report;
mod format;
mod law_report;
mod project_report;
mod report;

use std::process::ExitCode;

use casivell_core::{Money, TaxYear};
use casivell_income::{
    AssessmentLaw, Contributions, Employee, assess, capital_income_tax, taxable_income,
};
use casivell_lawdata::{DeductionParameters, ExtraordinaryBurdenParameters, SocialParameters};
use casivell_payroll::{Employment, HealthCover, PayPeriod, PayrollLaw, compare_classes, net_pay};
use casivell_projection::resolve;
use casivell_sim::{Household, SimulationConfig, simulate};
use casivell_social::Insured;
use casivell_tax::FilingStatus;

use args::Request;

/// Usage text, printed for `--help` and on any argument error.
const USAGE: &str = "\
casivell — German payroll and net pay, computed from statute

USAGE
  casivell --gross <amount> --class <1-6> [options]     a payslip
  casivell law --year <year> [--inflation <p>] [--wage-growth <p>]
  casivell project --gross <amount> --class <1-6> --expenses <amount> [options]

The `law` form prints the statutory parameters for a year. Past 2026 no statute
exists, so the figures are projected from explicit assumptions and labelled as
such — see --inflation and --wage-growth.

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

PROJECT OPTIONS
      --expenses <amount> Monthly expenses (required)
      --part-time <spec>  FROM:UNTIL:PERCENT in years, e.g. 3:8:60
      --break <spec>      Unpaid leave, FROM:UNTIL in years, e.g. 5:6
      --raise <spec>      YEAR:AMOUNT, e.g. 15:8000
      --one-off <spec>    YEAR:AMOUNT, e.g. 5:-60000 for a deposit
      --child-born <year>           A birth; credits Kindererziehungszeit
      --parental-leave <spec>       FROM:MONTHS[:PERCENT], e.g. 2:14 or 2:14:50
      --parental-leave-plus <spec>  The same, drawing ElterngeldPlus
      --years <n>         Horizon in years (default 40, max 70)
      --real              Show figures in today's purchasing power
      --pay-growth <p>    Annual growth in this household's pay (default 0,0)
      --return <p>        Annual nominal return on wealth (default 0,0)
      --inflation <p>     Annual price inflation (default 2,0)
      --wage-growth <p>   Annual wage growth (default 2,8)

ASSESS OPTIONS
      --work-expenses <a> Actual Werbungskosten, if above the Pauschbetrag
      --donations <a>     Other Sonderausgaben (§§ 10–10b)
      --capital <a>       Gross capital income for the year (§ 20)
      --benefits <a>      Tax-free wage-replacement benefits (§ 32b)
      --medical <a>       Extraordinary costs under § 33 (medical, funeral, …)
      --disability <gdb>  Grad der Behinderung, 20–100 (§ 33b Abs. 3)
      --helpless          Hilflos, blind or taubblind (§ 33b Abs. 3 Satz 3)
      --care-grade <n>    Pflegegrad of someone cared for, 2–5 (§ 33b Abs. 6)

CLASSES OPTIONS
      --partner <amount>  The second spouse's monthly gross (required)

LAW OPTIONS
      --inflation <p>     Annual price inflation for projection (default 2,0)
      --wage-growth <p>   Annual wage growth for projection (default 2,8)

EXAMPLES
  casivell --gross 4500 --class 1
  casivell assess --gross 5000 --class 1 --children 1 --capital 3000
  casivell classes --gross 5000 --partner 1800 --class 4
  casivell law --year 2026
  casivell law --year 2060 --inflation 3,0 --wage-growth 3,5
  casivell project --gross 4500 --class 1 --expenses 2500 --return 5,0 --real
  casivell project --gross 4500 --class 1 --expenses 2500 --part-time 3:8:60
  casivell project --gross 4500 --class 1 --expenses 2500 --break 5:6 --raise 15:8000
  casivell project --gross 4000 --class 1 --expenses 1800 --child-born 2 --parental-leave 2:14
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

    let outcome = match argv.first().map(String::as_str) {
        Some("assess") => run_assess(argv.into_iter().skip(1).collect()),
        Some("classes") => run_classes(argv.into_iter().skip(1).collect()),
        Some("law") => run_law(argv.into_iter().skip(1).collect()),
        Some("project") => run_project(argv.into_iter().skip(1).collect()),
        _ => run(argv),
    };

    match outcome {
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

/// The monthly gross equivalent to a figure given for `period`.
///
/// The assessment form works in whole years, so a salary quoted annually or monthly has to
/// reach it as one steady month. Refuses anything the PAP itself refuses.
fn monthly_equivalent(gross: Money, period: PayPeriod) -> Result<Money, casivell_core::MoneyError> {
    gross.div_int(period.months(), casivell_core::Rounding::HalfUp)
}

/// Every enacted parameter set the assessment needs, resolved together.
///
/// One `Err` per missing set would repeat the same sentence four times; resolving them here
/// keeps `run_assess` about the assessment rather than about lookups.
fn enacted_law(
    year: TaxYear,
    displayed: u16,
) -> Result<
    (
        PayrollLaw,
        DeductionParameters,
        SocialParameters,
        ExtraordinaryBurdenParameters,
    ),
    String,
> {
    let missing = |what: &str| format!("no enacted {what} parameters for {displayed}.");
    Ok((
        PayrollLaw::for_year(year).map_err(|_| missing("payroll"))?,
        DeductionParameters::for_year(year).map_err(|_| missing("deduction"))?,
        SocialParameters::for_year(year).map_err(|_| missing("social insurance"))?,
        ExtraordinaryBurdenParameters::for_year(year).map_err(|_| missing("§§ 33/33b"))?,
    ))
}

/// A year's employee facts, from twelve identical months.
fn annual_employee(
    request: &args::AssessRequest,
    pay: &casivell_payroll::NetPay,
    monthly_gross: Money,
    social: &SocialParameters,
) -> Result<Employee, casivell_core::MoneyError> {
    Ok(Employee {
        gross_annual: monthly_gross.mul_int(12)?,
        work_expenses: request.work_expenses,
        contributions: Contributions::from_social(
            &pay.monthly_contributions,
            social,
            request.base.supplementary_rate,
            12,
        )?,
        church_tax_paid: pay.church_tax.mul_int(12)?,
        other_special_expenses: request.other_special_expenses,
        wage_replacement_benefits: request.benefits,
        extraordinary: request.extraordinary,
        children: request.base.children,
    })
}

/// The § 32d computation, where there is capital income to run it on.
fn capital_or_none(
    request: &args::AssessRequest,
    assessment: &casivell_income::Assessment,
    filing: FilingStatus,
    church: Option<casivell_lawdata::Bundesland>,
    law: &AssessmentLaw,
) -> Result<Option<casivell_income::CapitalIncomeTax>, casivell_core::MoneyError> {
    if request.capital_income.is_zero() {
        return Ok(None);
    }
    Ok(Some(capital_income_tax(
        request.capital_income,
        assessment.taxable_income,
        filing,
        church,
        law,
    )?))
}

/// Runs an annual assessment for one steady salary and renders the § 2 chain.
fn run_assess(argv: Vec<String>) -> Result<String, String> {
    let request = args::parse_assess(argv).map_err(|e| e.to_string())?;
    let base = request.base;

    let year = TaxYear::new(base.year).map_err(|_| {
        format!(
            "{} is outside the representable range {}–{}.",
            base.year,
            TaxYear::FIRST_VERIFIED.get(),
            TaxYear::LAST_REPRESENTABLE.get(),
        )
    })?;
    let (law, deductions, social, burden) = enacted_law(year, base.year)?;

    let employment = build_employment(&base).map_err(|e| e.to_string())?;
    let monthly_gross = monthly_equivalent(base.gross, base.period).map_err(|e| e.to_string())?;

    // A full year at a steady salary: twelve identical months, which is what an assessment
    // form can assume when it is given one figure. The report says so.
    let pay =
        net_pay(monthly_gross, PayPeriod::Month, &employment, &law).map_err(|e| e.to_string())?;
    let withheld = pay
        .income_tax
        .add(pay.solidarity_surcharge)
        .and_then(|t| t.add(pay.church_tax))
        .and_then(|t| t.mul_int(12))
        .map_err(|e| e.to_string())?;

    let employee =
        annual_employee(&request, &pay, monthly_gross, &social).map_err(|e| e.to_string())?;
    // Class III is a single-earner married couple, whose whole income this form has. Every
    // other class is either an individual or a household the form cannot see all of, so it
    // assesses individually and the notes say the figure is an estimate.
    let filing = match base.tax_class {
        casivell_lawdata::TaxClass::Class3 => FilingStatus::JointSplitting,
        _ => FilingStatus::Individual,
    };
    let income =
        taxable_income(&employee, filing, &deductions, &burden).map_err(|e| e.to_string())?;

    let assessment_law = AssessmentLaw {
        tariff: law.tariff,
        solidarity: law.solidarity,
        church: law.church,
        deductions,
        burden,
    };
    let church = if base.church { Some(base.land) } else { None };
    let assessment = assess(
        &income,
        filing,
        church,
        u16::from(base.children).saturating_mul(10),
        withheld,
        &assessment_law,
    )
    .map_err(|e| e.to_string())?;

    let capital = capital_or_none(&request, &assessment, filing, church, &assessment_law)
        .map_err(|e| e.to_string())?;

    assess_report::render(&assess_report::AssessmentReport {
        year: base.year,
        income,
        assessment,
        capital,
        benefits: request.benefits,
    })
    .map_err(|e| e.to_string())
}

/// Compares the tax-class arrangements available to a married couple.
fn run_classes(argv: Vec<String>) -> Result<String, String> {
    let request = args::parse_classes(argv).map_err(|e| e.to_string())?;
    let base = request.base;

    let year = TaxYear::new(base.year).map_err(|_| {
        format!(
            "{} is outside the representable range {}–{}.",
            base.year,
            TaxYear::FIRST_VERIFIED.get(),
            TaxYear::LAST_REPRESENTABLE.get(),
        )
    })?;
    let law = PayrollLaw::for_year(year)
        .map_err(|_| format!("no enacted payroll parameters for {}.", base.year))?;
    let employment = build_employment(&base).map_err(|e| e.to_string())?;

    // Which salary is the higher one is the caller's to state only in the sense that III/V
    // is priced with the higher earner in III; swapping them here rather than refusing keeps
    // the report meaningful whichever order they were given in.
    let (higher, lower) = if base.gross >= request.partner_gross {
        (base.gross, request.partner_gross)
    } else {
        (request.partner_gross, base.gross)
    };

    let comparison = compare_classes(higher, lower, &employment, &employment, &law)
        .map_err(|e| e.to_string())?;
    classes_report::render(&comparison, higher, lower, base.year).map_err(|e| e.to_string())
}

/// Renders the statutory parameters for a year, projecting past the last enacted one.
fn run_law(argv: Vec<String>) -> Result<String, String> {
    let (year_value, assumptions) = args::parse_law(argv).map_err(|e| e.to_string())?;

    let year = TaxYear::new(year_value).map_err(|_| {
        format!(
            "{year_value} is outside the representable range {}–{}.",
            TaxYear::FIRST_VERIFIED.get(),
            TaxYear::LAST_REPRESENTABLE.get(),
        )
    })?;

    let law = resolve(year, &assumptions).map_err(|e| e.to_string())?;
    law_report::render(&law, &assumptions).map_err(|e| e.to_string())
}

/// Projects a household forward and renders the result.
fn run_project(argv: Vec<String>) -> Result<String, String> {
    let request = args::parse_project(argv).map_err(|e| e.to_string())?;
    let base = request.base;

    let year = TaxYear::new(base.year).map_err(|_| {
        format!(
            "{} is outside the representable range {}–{}.",
            base.year,
            TaxYear::FIRST_VERIFIED.get(),
            TaxYear::LAST_REPRESENTABLE.get(),
        )
    })?;

    let employment = build_employment(&base).map_err(|e| e.to_string())?;
    let mut household =
        Household::starting_fresh(year, 1, employment, base.gross, request.monthly_expenses)
            .map_err(|e| e.to_string())?;
    household.annual_pay_growth = request.pay_growth;
    household.schedule = request.schedule;
    // Expenses default to growing with prices, which is the assumption a user who has not
    // thought about it would want; a flat nominal spend over forty years is not a scenario
    // anyone means.
    household.annual_expense_growth = request.assumptions.price_inflation();

    let config = SimulationConfig {
        horizon: request.horizon,
        assumptions: request.assumptions,
        investment_return: request.investment_return,
        basis: request.basis,
    };

    let mut sink = project_report::YearlySink::new();
    simulate(&household, &config, &mut sink).map_err(|e| e.to_string())?;
    project_report::render(&household, &config, &sink).map_err(|e| e.to_string())
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
            TaxYear::FIRST_VERIFIED.get(),
            TaxYear::LAST_VERIFIED.get(),
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
    use super::{run, run_law, run_project};

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The `law` form must work for an enacted year and label it as law.
    #[test]
    fn the_law_form_renders_an_enacted_year() {
        let text = run_law(argv(&["--year", "2026"])).expect("succeeds");
        assert!(text.contains("ENACTED LAW"));
        assert!(text.contains("12.348,00"));
    }

    /// It must also work *past* the last enacted year — that is the whole point — and
    /// must lead with the projection warning rather than burying it.
    #[test]
    fn the_law_form_projects_past_the_last_enacted_year() {
        let text = run_law(argv(&["--year", "2060"])).expect("succeeds");
        assert!(text.contains("PROJECTED"));
        assert!(text.contains("NOT ENACTED LAW"));
        // The Grundfreibetrag must have grown well past the enacted 12 348 EUR.
        assert!(!text.contains("Grundfreibetrag                          12.348,00"));
    }

    /// The assumptions must be honoured, not merely accepted: a higher inflation
    /// assumption must produce a visibly higher Grundfreibetrag.
    #[test]
    fn the_projection_assumptions_change_the_result() {
        let low = run_law(argv(&["--year", "2050", "--inflation", "1,0"])).expect("a");
        let high = run_law(argv(&["--year", "2050", "--inflation", "4,0"])).expect("b");
        assert_ne!(low, high, "the inflation assumption had no effect");
        assert!(low.contains("1,00 %"));
        assert!(high.contains("4,00 %"));
    }

    /// A year past the coherence horizon must be refused with the reason, not returned.
    #[test]
    fn an_incoherent_horizon_is_refused_with_its_reason() {
        let err = run_law(argv(&["--year", "2120"])).expect_err("refuses");
        assert!(
            err.contains("45 %"),
            "the message should explain the tariff broke: {err}"
        );
    }

    #[test]
    fn the_law_form_requires_a_year() {
        assert!(run_law(argv(&[])).is_err());
        assert!(run_law(argv(&["--inflation", "2,0"])).is_err());
    }

    #[test]
    fn a_minimal_invocation_produces_a_report() {
        let text = run(argv(&["--gross", "4500", "--class", "1"])).expect("succeeds");
        assert!(text.contains("Nettoentgelt"));
    }

    /// A year with no statutory data must be refused with an explanation, never
    /// answered with a nearby year's figures.
    ///
    /// Since `TaxYear` was widened to make projection expressible, a future year is
    /// now *representable* and is refused one step later — at the law lookup. Both
    /// paths must still refuse, and say why.
    #[test]
    fn a_year_without_data_is_refused_with_an_explanation() {
        // Before the first transcribed statute: refused at year construction.
        let err = run(argv(&["--gross", "4500", "-c", "1", "-y", "2019"])).expect_err("refuses");
        assert!(err.contains("2019"), "the message omits the year: {err}");
        assert!(
            err.contains("refuses"),
            "the message omits the reason: {err}"
        );

        // Beyond it: representable, but no Programmablaufplan to withhold under.
        let err = run(argv(&["--gross", "4500", "-c", "1", "-y", "2040"])).expect_err("refuses");
        assert!(err.contains("2040"), "the message omits the year: {err}");
        assert!(
            err.contains("Programmablaufplan"),
            "the message should name what is missing: {err}"
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

    /// The `project` form must run end to end and label where enacted law stops.
    #[test]
    fn the_project_form_runs_and_marks_the_end_of_enacted_law() {
        let text = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "10",
        ]))
        .expect("succeeds");
        assert!(text.contains("Haushaltsprojektion"));
        assert!(text.contains("enacted law ends here"));
        assert!(text.contains("2026"));
        assert!(text.contains("2035"));
    }

    /// A forty-year projection — the horizon the product promises — must work by default.
    #[test]
    fn the_project_form_defaults_to_forty_years() {
        let text = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
        ]))
        .expect("succeeds");
        assert!(text.contains("40 years from 2026"));
        assert!(text.contains("2065"), "the final year is missing");
    }

    /// `--real` must change the figures, not merely the label.
    #[test]
    fn the_real_basis_changes_the_projection() {
        let nominal = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "25",
        ]))
        .expect("a");
        let real = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "25",
            "--real",
        ]))
        .expect("b");
        assert_ne!(nominal, real);
        assert!(real.contains("today's purchasing power"));
    }

    /// The household flags shared with the payslip form must reach the projection, so the
    /// two forms cannot describe different people from the same arguments.
    #[test]
    fn the_shared_household_flags_reach_the_projection() {
        let childless = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2000",
            "--years",
            "5",
        ]))
        .expect("a");
        let parent = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2000",
            "--years",
            "5",
            "--children",
            "2",
        ]))
        .expect("b");
        // Children lower the care rate, so net pay and therefore wealth must differ.
        assert_ne!(childless, parent);

        let bavarian_churchgoer = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2000",
            "--years",
            "5",
            "--state",
            "BY",
            "--church",
        ]))
        .expect("c");
        assert_ne!(childless, bavarian_churchgoer);
    }

    /// Expenses are required: a projection without them would silently assume a household
    /// spends nothing, which is not a scenario anyone means.
    #[test]
    fn the_project_form_requires_expenses() {
        let err = run_project(argv(&["--gross", "4500", "--class", "1"])).expect_err("refuses");
        assert!(err.contains("--expenses"), "unhelpful message: {err}");
    }

    /// A horizon past the kernel's limit must be refused with the limit named.
    #[test]
    fn an_excessive_horizon_is_refused() {
        let err = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "200",
        ]))
        .expect_err("refuses");
        assert!(
            err.contains("70"),
            "the message should name the limit: {err}"
        );
    }

    /// The investment return must actually compound: a higher return must leave more wealth.
    #[test]
    fn a_higher_return_leaves_more_wealth() {
        let low = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "30",
            "--return",
            "1,0",
        ]))
        .expect("a");
        let high = run_project(argv(&[
            "--gross",
            "4500",
            "--class",
            "1",
            "--expenses",
            "2500",
            "--years",
            "30",
            "--return",
            "7,0",
        ]))
        .expect("b");
        assert_ne!(low, high);
        assert!(high.contains("7,00 %"));
    }
}
