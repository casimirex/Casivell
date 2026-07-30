//! Rendering a household projection.
//!
//! The kernel streams months and keeps none of them, so this collects only what it prints:
//! one row per year, at the anniversary. A forty-year run is 480 months and nobody reads
//! 480 rows.
//!
//! The row where enacted law ends is marked, because that is the most important thing on
//! the page and a footnote would bury it.

use core::fmt::Write as _;

use casivell_core::{Money, MoneyError};
use casivell_lawdata::DataStatus;
use casivell_sim::{Basis, Household, MonthSnapshot, SimulationConfig, Sink};

use crate::format::{euro, percent};

/// One row of the table: the state at a year's end.
#[derive(Debug, Clone, Copy)]
pub(crate) struct YearRow {
    year: u16,
    gross: Money,
    net: Money,
    savings: Money,
    wealth: Money,
    pension_points: i64,
    accrued_pension: Money,
    status: DataStatus,
}

/// A sink that keeps one snapshot per year.
///
/// The whole timeline never exists at once, which is the streaming design working as
/// intended rather than an inconvenience.
pub(crate) struct YearlySink {
    rows: [Option<YearRow>; Self::MAX_ROWS],
    count: usize,
    months: u32,
}

impl YearlySink {
    /// Enough rows for the longest horizon the kernel permits.
    const MAX_ROWS: usize = 71;

    pub(crate) const fn new() -> Self {
        Self {
            rows: [None; Self::MAX_ROWS],
            count: 0,
            months: 0,
        }
    }

    /// The rows collected, in order.
    pub(crate) fn rows(&self) -> impl Iterator<Item = &YearRow> {
        self.rows.iter().take(self.count).flatten()
    }

    /// Months actually simulated.
    pub(crate) const fn months(&self) -> u32 {
        self.months
    }
}

impl Sink for YearlySink {
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        self.months = self.months.saturating_add(1);

        // Keep the twelfth month of each simulated year, and always the very last one, so a
        // horizon that is not a whole number of years still shows its end state.
        let is_year_end = snapshot.month_index % 12 == 11;
        if !is_year_end {
            return true;
        }
        if let Some(slot) = self.rows.get_mut(self.count) {
            *slot = Some(YearRow {
                year: snapshot.year,
                gross: snapshot.gross,
                net: snapshot.net,
                savings: snapshot.savings,
                wealth: snapshot.wealth,
                pension_points: snapshot.pension_points.micro(),
                accrued_pension: snapshot.accrued_pension,
                status: snapshot.law_status,
            });
            self.count = self.count.saturating_add(1);
        }
        true
    }
}

/// Renders the projection.
///
/// # Errors
///
/// [`MoneyError`] if a figure cannot be formatted.
pub(crate) fn render(
    household: &Household,
    config: &SimulationConfig,
    sink: &YearlySink,
) -> Result<String, MoneyError> {
    let mut out = String::with_capacity(4_096);
    write_header(&mut out, household, config, sink)?;
    write_table(&mut out, sink)?;
    write_notes(&mut out, config);
    Ok(out)
}

fn write_header(
    out: &mut String,
    household: &Household,
    config: &SimulationConfig,
    sink: &YearlySink,
) -> Result<(), MoneyError> {
    let basis = match config.basis {
        Basis::Nominal => "nominal (euro of the day)",
        Basis::Real => "real (in today's purchasing power)",
    };
    let years = sink.months() / 12;
    let _ = writeln!(out, "\nCasivell — Haushaltsprojektion");
    let _ = writeln!(
        out,
        "  {} years from {} · {basis}",
        years,
        household.start_year.get()
    );
    let _ = writeln!(
        out,
        "  Assumptions: {} prices, {} wages, {} investment return, {} pay growth",
        percent(config.assumptions.price_inflation())?,
        percent(config.assumptions.wage_growth())?,
        percent(config.investment_return)?,
        percent(household.annual_pay_growth)?,
    );
    let _ = writeln!(out);
    Ok(())
}

fn write_table(out: &mut String, sink: &YearlySink) -> Result<(), MoneyError> {
    let _ = writeln!(
        out,
        "  Year   Gross/mo      Net/mo   Saved/mo        Wealth    Points   Pension/mo"
    );
    let _ = writeln!(out, "  {}", "─".repeat(74));

    let mut law_ended = false;
    for row in sink.rows() {
        // Mark the transition once, on the first projected row.
        if !law_ended && row.status != DataStatus::Enacted {
            let _ = writeln!(
                out,
                "  {}  enacted law ends here; rows below are projected",
                "┈".repeat(8)
            );
            law_ended = true;
        }
        let _ = writeln!(
            out,
            "  {:>4}  {:>9}  {:>10}  {:>9}  {:>12}  {:>8}  {:>11}",
            row.year,
            euro(row.gross)?,
            euro(row.net)?,
            euro(row.savings)?,
            euro(row.wealth)?,
            points(row.pension_points),
            euro(row.accrued_pension)?,
        );
    }
    let _ = writeln!(out);
    Ok(())
}

/// Formats Entgeltpunkte with two decimals, as the DRV reports them.
fn points(micro: i64) -> String {
    let hundredths = micro.saturating_add(5_000) / 10_000;
    let whole = hundredths / 100;
    let fraction = (hundredths % 100).unsigned_abs();
    format!("{whole},{fraction:02}")
}

fn write_notes(out: &mut String, config: &SimulationConfig) {
    let _ = writeln!(out, "  Notes");
    if config.basis == Basis::Nominal {
        let _ = writeln!(
            out,
            "  · Nominal figures. Pass --real to see them in today's purchasing power."
        );
    }
    let _ = writeln!(
        out,
        "  · Rows past the marked line rest on projected statutory parameters, not law."
    );
    let _ = writeln!(
        out,
        "  · Points are Entgeltpunkte; Pension/mo is what that record would pay at the"
    );
    let _ = writeln!(
        out,
        "    Rentenwert then in force, with no early-claim reduction applied."
    );
    let _ = writeln!(
        out,
        "  · Not modelled: children, property, career breaks, capital income, one-off"
    );
    let _ = writeln!(
        out,
        "    payments, or the annual assessment's refund. One steady employment only."
    );
    let _ = writeln!(out, "\n  Not tax or investment advice.\n");
}

#[cfg(test)]
mod tests {
    use super::{YearlySink, render};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_payroll::{Employment, HealthCover};
    use casivell_sim::{Basis, Horizon, Household, SimulationConfig, simulate};
    use casivell_social::Insured;

    fn run(years: u32, basis: Basis) -> String {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        let employment = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap();
        let household = Household::starting_fresh(
            TaxYear::new(2026).unwrap(),
            1,
            employment,
            Money::from_euro(4_500).unwrap(),
            Money::from_euro(2_500).unwrap(),
        )
        .unwrap();
        let config = SimulationConfig::conservative(Horizon::years(years).unwrap(), basis);

        let mut sink = YearlySink::new();
        simulate(&household, &config, &mut sink).expect("simulates");
        render(&household, &config, &sink).expect("renders")
    }

    #[test]
    fn the_table_has_one_row_per_year() {
        let text = run(10, Basis::Nominal);
        for year in 2026_u16..=2035 {
            assert!(text.contains(&year.to_string()), "{year} is missing");
        }
    }

    /// The transition out of enacted law must be marked in the table, not left to a
    /// footnote. It is the most important thing on the page.
    #[test]
    fn the_end_of_enacted_law_is_marked_in_the_table() {
        let text = run(5, Basis::Nominal);
        assert!(text.contains("enacted law ends here"));
        // And it must appear exactly once, not on every projected row.
        assert_eq!(text.matches("enacted law ends here").count(), 1);
    }

    #[test]
    fn the_header_states_the_basis_and_every_assumption() {
        let nominal = run(5, Basis::Nominal);
        assert!(nominal.contains("nominal"));
        assert!(
            nominal.contains("2,00 %"),
            "the price assumption is missing"
        );
        assert!(nominal.contains("2,80 %"), "the wage assumption is missing");
        assert!(nominal.contains("--real"), "the real hint is missing");

        let real = run(5, Basis::Real);
        assert!(real.contains("real (in today's purchasing power)"));
        assert!(
            !real.contains("--real"),
            "the hint is pointless in real mode"
        );
    }

    /// Real figures must be visibly lower than nominal at the end of a long run.
    #[test]
    fn the_real_basis_shows_lower_figures_than_the_nominal_one() {
        assert_ne!(run(25, Basis::Nominal), run(25, Basis::Real));
    }

    #[test]
    fn the_notes_state_what_is_not_modelled() {
        let text = run(5, Basis::Nominal);
        assert!(text.contains("Not modelled"));
        assert!(text.contains("Entgeltpunkte"));
        assert!(text.contains("Not tax or investment advice"));
    }

    /// The sink must survive the longest horizon the kernel permits without dropping rows.
    #[test]
    fn the_longest_horizon_fits_the_sink() {
        let text = run(70, Basis::Nominal);
        assert!(text.contains("2095"), "the final year is missing");
    }
}
