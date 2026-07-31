//! Projecting a household forward, across the ABI.
//!
//! # Why this one is set up rather than called
//!
//! The payslip takes eight numbers and a C ABI passes eight numbers happily. A projection
//! takes a household, a horizon and four growth assumptions, and a function of thirteen
//! positional `i64`s is a function nobody can call correctly twice.
//!
//! So parameters are **named and set one at a time**, then the run is asked for. It reads at
//! the call site, it extends without changing a signature, and a caller that forgets one gets
//! the documented default rather than whatever happened to be in argument nine.
//!
//! # And why the result is a table rather than a value
//!
//! A projection's answer is a series: one row per year, several figures each. The kernel
//! streams months and keeps none of them, so this collects the year ends into a bounded array
//! — the same thing the CLI's report sink does, for the same reason.

use core::cell::RefCell;

use casivell_core::{Money, Rate, TaxYear};
use casivell_lawdata::Bundesland;
use casivell_payroll::{Employment, HealthCover};
use casivell_projection::Assumptions;
use casivell_sim::{Basis, Horizon, Household, MonthSnapshot, SimulationConfig, Sink, simulate};
use casivell_social::Insured;

use crate::error;

/// Parameters, for [`casivell_project_set`].
///
/// Every value crosses as an `i64`: money in cents, rates in parts per million, counts and
/// flags as themselves.
pub mod param {
    /// Monthly gross pay, in cents.
    pub const GROSS: i32 = 0;
    /// The tax year the projection starts in.
    pub const YEAR: i32 = 1;
    /// Lohnsteuerklasse, one to six.
    pub const TAX_CLASS: i32 = 2;
    /// Index into `Bundesland::ALL`.
    pub const LAND: i32 = 3;
    /// Age in whole years at the start.
    pub const AGE: i32 = 4;
    /// Children under 25.
    pub const CHILDREN: i32 = 5;
    /// Elterneigenschaft: one or zero.
    pub const IS_PARENT: i32 = 6;
    /// Whether church tax is levied: one or zero.
    pub const CHURCH: i32 = 7;
    /// The fund's supplementary rate, in parts per million.
    pub const SUPPLEMENTARY_RATE: i32 = 8;
    /// Monthly expenses, in cents.
    pub const EXPENSES: i32 = 9;
    /// Horizon in whole years.
    pub const YEARS: i32 = 10;
    /// Annual nominal return on wealth, in parts per million.
    pub const INVESTMENT_RETURN: i32 = 11;
    /// Annual growth in this household's own pay, in parts per million.
    pub const PAY_GROWTH: i32 = 12;
    /// Annual price inflation, in parts per million.
    pub const INFLATION: i32 = 13;
    /// Annual wage growth, in parts per million.
    pub const WAGE_GROWTH: i32 = 14;
    /// One past the last valid parameter.
    pub const COUNT: i32 = 15;
}

/// Figures available per year, for [`casivell_project_value`].
pub mod row {
    /// The calendar year.
    pub const YEAR: i32 = 0;
    /// Monthly gross at that year's end, in cents.
    pub const GROSS: i32 = 1;
    /// Monthly net.
    pub const NET: i32 = 2;
    /// Monthly savings.
    pub const SAVED: i32 = 3;
    /// Financial wealth at the year's end.
    pub const WEALTH: i32 = 4;
    /// Wealth plus any property, less its mortgage.
    pub const NET_WORTH: i32 = 5;
    /// Entgeltpunkte accrued, in millionths.
    pub const PENSION_POINTS: i32 = 6;
    /// The monthly pension that record would pay at the Rentenwert then in force.
    pub const ACCRUED_PENSION: i32 = 7;
    /// Whether the year rests on enacted law: one or zero.
    pub const IS_ENACTED: i32 = 8;
    /// One past the last valid field.
    pub const COUNT: i32 = 9;
}

/// The longest projection this ABI will return rows for.
///
/// Seventy years is the kernel's own limit, so a bounded array of that size can always hold
/// the answer and the browser never has to ask how much to allocate.
const MAX_ROWS: usize = 71;

/// One year's figures, as the ABI reports them.
#[derive(Debug, Clone, Copy, Default)]
struct Row {
    year: i64,
    gross: i64,
    net: i64,
    saved: i64,
    wealth: i64,
    net_worth: i64,
    pension_points: i64,
    accrued_pension: i64,
    is_enacted: i64,
}

/// A sink keeping one row per year, as the CLI's report does.
struct Yearly {
    rows: [Row; MAX_ROWS],
    count: usize,
}

impl Sink for Yearly {
    fn accept(&mut self, snapshot: &MonthSnapshot) -> bool {
        if snapshot.month_index % 12 != 11 {
            return true;
        }
        if let Some(slot) = self.rows.get_mut(self.count) {
            *slot = Row {
                year: i64::from(snapshot.year),
                gross: snapshot.gross.cents(),
                net: snapshot.net.cents(),
                saved: snapshot.savings.cents(),
                wealth: snapshot.wealth.cents(),
                net_worth: snapshot.net_worth.cents(),
                pension_points: snapshot.pension_points.micro(),
                accrued_pension: snapshot.accrued_pension.cents(),
                is_enacted: i64::from(snapshot.law_status.is_binding_law()),
            };
            self.count = self.count.saturating_add(1);
        }
        true
    }
}

thread_local! {
    /// The parameters set so far, and the rows of the last successful run.
    static PARAMS: RefCell<[i64; param::COUNT as usize]> =
        const { RefCell::new(DEFAULTS) };
    static ROWS: RefCell<Option<Yearly>> = const { RefCell::new(None) };
}

/// The defaults a caller gets for anything it does not set.
///
/// Chosen to be the commonest case rather than the cheapest: a childless thirty-year-old in
/// class I, forty years, and the statutory growth assumptions the projection crate uses.
/// Written positionally, in `param` order, so the defaults read as one table rather than as
/// fifteen assignments. `the_defaults_line_up_with_their_names` checks the order, because a
/// literal like this is exactly the kind that survives a reordering of the constants above it.
const DEFAULTS: [i64; param::COUNT as usize] = [
    0,      // GROSS — no default; a projection of nothing is the caller's to ask for
    2026,   // YEAR
    1,      // TAX_CLASS
    9,      // LAND — Nordrhein-Westfalen, the most populous
    30,     // AGE
    0,      // CHILDREN
    0,      // IS_PARENT
    0,      // CHURCH
    29_000, // SUPPLEMENTARY_RATE — 2,9 %, the published average
    0,      // EXPENSES
    40,     // YEARS
    0,      // INVESTMENT_RETURN — zero is not neutral, which is why it must be chosen
    0,      // PAY_GROWTH — likewise
    20_000, // INFLATION — 2,0 %, the projection crate's own default
    28_000, // WAGE_GROWTH — 2,8 %
];

/// Restores every parameter to its default.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_project_reset() {
    PARAMS.with(|slot| *slot.borrow_mut() = DEFAULTS);
    ROWS.with(|slot| *slot.borrow_mut() = None);
}

/// Sets one parameter. Returns `0`, or [`error::FIELD`] for an unknown one.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_project_set(which: i32, value: i64) -> i32 {
    let Ok(index) = usize::try_from(which) else {
        return error::FIELD;
    };
    PARAMS.with(|slot| match slot.borrow_mut().get_mut(index) {
        Some(entry) => {
            *entry = value;
            0
        }
        None => error::FIELD,
    })
}

/// Runs the projection. Returns `0` or an [`error`] code.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_project_run() -> i32 {
    let params = PARAMS.with(|slot| *slot.borrow());
    match run(&params) {
        Ok(rows) => {
            ROWS.with(|slot| *slot.borrow_mut() = Some(rows));
            0
        }
        Err(code) => {
            ROWS.with(|slot| *slot.borrow_mut() = None);
            code
        }
    }
}

/// How many year rows the last run produced, or `0`.
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_project_years() -> i32 {
    ROWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|rows| i32::try_from(rows.count).ok())
            .unwrap_or(0)
    })
}

/// One figure from one year of the last run, or [`error::FIELD`].
#[expect(
    unsafe_code,
    reason = "the export attribute only; the function body contains no unsafe operation"
)]
#[unsafe(no_mangle)]
pub extern "C" fn casivell_project_value(index: i32, field: i32) -> i64 {
    ROWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(rows) = borrowed.as_ref() else {
            return i64::from(error::FIELD);
        };
        let Some(year) = usize::try_from(index)
            .ok()
            .filter(|position| *position < rows.count)
            .and_then(|position| rows.rows.get(position))
        else {
            return i64::from(error::FIELD);
        };
        match field {
            row::YEAR => year.year,
            row::GROSS => year.gross,
            row::NET => year.net,
            row::SAVED => year.saved,
            row::WEALTH => year.wealth,
            row::NET_WORTH => year.net_worth,
            row::PENSION_POINTS => year.pension_points,
            row::ACCRUED_PENSION => year.accrued_pension,
            row::IS_ENACTED => year.is_enacted,
            _ => i64::from(error::FIELD),
        }
    })
}

/// Reads a parameter out of the array, which is indexed by the `param` constants.
fn get(params: &[i64; param::COUNT as usize], which: i32) -> i64 {
    usize::try_from(which)
        .ok()
        .and_then(|index| params.get(index))
        .copied()
        .unwrap_or(0)
}

/// Builds the employment the projection runs on.
fn employment_from(params: &[i64; param::COUNT as usize]) -> Result<Employment, i32> {
    let land = *Bundesland::ALL
        .get(usize::try_from(get(params, param::LAND)).map_err(|_| error::LAND)?)
        .ok_or(error::LAND)?;
    let class = crate::tax_class(
        i32::try_from(get(params, param::TAX_CLASS)).map_err(|_| error::TAX_CLASS)?,
    )?;

    let age = u8::try_from(get(params, param::AGE)).map_err(|_| error::INPUT)?;
    let children = u8::try_from(get(params, param::CHILDREN)).map_err(|_| error::INPUT)?;
    let supplementary =
        Rate::from_ppm(get(params, param::SUPPLEMENTARY_RATE)).map_err(|_| error::INPUT)?;

    let insured = Insured::new(
        age,
        get(params, param::IS_PARENT) != 0,
        children,
        land,
        Some(supplementary),
    )
    .map_err(|_| error::INPUT)?;
    Employment::new(
        insured,
        class,
        u16::from(children).saturating_mul(10),
        HealthCover::Statutory {
            supplementary_rate: supplementary,
        },
        (get(params, param::CHURCH) != 0).then_some(land),
    )
    .map_err(|_| error::INPUT)
}

/// Builds the household and runs the kernel.
fn run(params: &[i64; param::COUNT as usize]) -> Result<Yearly, i32> {
    let year = u16::try_from(get(params, param::YEAR))
        .ok()
        .and_then(|value| TaxYear::new(value).ok())
        .ok_or(error::YEAR)?;
    let employment = employment_from(params)?;

    let mut household = Household::starting_fresh(
        year,
        1,
        employment,
        Money::from_cents(get(params, param::GROSS)).map_err(|_| error::INPUT)?,
        Money::from_cents(get(params, param::EXPENSES)).map_err(|_| error::INPUT)?,
    )
    .map_err(|_| error::INPUT)?;
    household.annual_pay_growth =
        Rate::from_ppm(get(params, param::PAY_GROWTH)).map_err(|_| error::INPUT)?;

    let assumptions = Assumptions::new(
        Rate::from_ppm(get(params, param::INFLATION)).map_err(|_| error::INPUT)?,
        Rate::from_ppm(get(params, param::WAGE_GROWTH)).map_err(|_| error::INPUT)?,
    )
    .map_err(|_| error::INPUT)?;
    // Expenses grow with prices unless a caller says otherwise, which is what someone who has
    // not thought about it means: a flat nominal spend over forty years is nobody's scenario.
    household.annual_expense_growth = assumptions.price_inflation();

    let config = SimulationConfig {
        horizon: Horizon::years(
            u32::try_from(get(params, param::YEARS)).map_err(|_| error::INPUT)?,
        )
        .map_err(|_| error::INPUT)?,
        assumptions,
        investment_return: Rate::from_ppm(get(params, param::INVESTMENT_RETURN))
            .map_err(|_| error::INPUT)?,
        basis: Basis::Nominal,
        property_growth: Rate::ZERO,
    };

    let mut sink = Yearly {
        rows: [Row::default(); MAX_ROWS],
        count: 0,
    };
    simulate(&household, &config, &mut sink).map_err(|_| error::ARITHMETIC)?;
    Ok(sink)
}

#[cfg(test)]
mod tests {
    use super::{
        casivell_project_reset, casivell_project_run, casivell_project_set, casivell_project_value,
        casivell_project_years, param, row,
    };
    use crate::error;

    /// Sets the minimum a projection needs and runs it.
    fn run_default() -> i32 {
        casivell_project_reset();
        casivell_project_set(param::GROSS, 500_000);
        casivell_project_set(param::EXPENSES, 200_000);
        casivell_project_run()
    }

    /// The defaults must line up with the names above them.
    ///
    /// `DEFAULTS` is written positionally, which is readable and exactly the kind of literal
    /// that survives a reordering of the constants it corresponds to. A default run starting
    /// in the right year, over the right horizon, in the right class is what would break.
    #[test]
    fn the_defaults_line_up_with_their_names() {
        assert_eq!(run_default(), 0);
        assert_eq!(casivell_project_years(), 40, "YEARS must default to forty");
        assert_eq!(
            casivell_project_value(0, row::YEAR),
            2026,
            "YEAR must default to 2026"
        );
        // Class I on 5 000 EUR nets around 3 100 EUR; class III would be visibly more, so an
        // off-by-one in the defaults would show here.
        let net = casivell_project_value(0, row::NET);
        assert!(
            (280_000..340_000).contains(&net),
            "the first year's net is {net} cents, which is not class I on 5 000 EUR"
        );
    }

    /// The series must be one row per year, in order, and the law status must cross over
    /// exactly where the enacted data ends.
    #[test]
    fn the_series_is_one_row_per_year_and_marks_where_law_ends() {
        assert_eq!(run_default(), 0);
        let years = casivell_project_years();
        assert_eq!(years, 40);

        for index in 0..years {
            assert_eq!(
                casivell_project_value(index, row::YEAR),
                2026 + i64::from(index)
            );
        }
        // Only the first year is enacted; everything after it is projected and says so.
        assert_eq!(casivell_project_value(0, row::IS_ENACTED), 1);
        assert_eq!(casivell_project_value(1, row::IS_ENACTED), 0);
        assert_eq!(casivell_project_value(years - 1, row::IS_ENACTED), 0);
    }

    /// Wealth accumulates while the household saves, and net worth equals it while there is
    /// no property — the browser draws both, and two identical lines would be a bug worth
    /// catching here rather than by eye.
    #[test]
    fn wealth_accumulates_and_net_worth_tracks_it_without_property() {
        casivell_project_reset();
        casivell_project_set(param::GROSS, 500_000);
        casivell_project_set(param::EXPENSES, 200_000);
        // Pay keeping pace with prices, so the household's savings rate holds. See the test
        // below for what happens when it does not.
        casivell_project_set(param::PAY_GROWTH, 20_000);
        assert_eq!(casivell_project_run(), 0);

        let mut previous = 0;
        for index in 0..casivell_project_years() {
            let wealth = casivell_project_value(index, row::WEALTH);
            assert!(wealth > previous, "wealth fell in row {index}");
            assert_eq!(
                casivell_project_value(index, row::NET_WORTH),
                wealth,
                "with no property the two must be identical"
            );
            previous = wealth;
        }
    }

    /// A household whose pay never rises while prices do eventually spends more than it
    /// earns, and its savings start to run down.
    ///
    /// This test exists because it caught the previous one out: the defaults have zero pay
    /// growth and expenses tracking inflation, and wealth peaks around the twenty-eighth year
    /// and falls thereafter. The engine was right and the assertion was wrong.
    ///
    /// It is worth an assertion of its own because a projection that only ever curved upward
    /// would be flattering nonsense, and a household reading this chart should see the turn.
    #[test]
    fn flat_pay_against_rising_prices_eventually_runs_wealth_down() {
        assert_eq!(run_default(), 0); // zero pay growth, expenses at 2 %
        let years = casivell_project_years();

        let wealth = |index: i32| casivell_project_value(index, row::WEALTH);
        let peak = (0..years).max_by_key(|index| wealth(*index)).expect("rows");

        assert!(
            peak > 0 && peak < years - 1,
            "wealth should turn somewhere inside the horizon, not at row {peak}"
        );
        assert!(wealth(years - 1) < wealth(peak), "and fall after the peak");
        // Savings go negative before wealth does, which is the earlier warning.
        assert!(casivell_project_value(years - 1, row::SAVED) < 0);
    }

    /// Entgeltpunkte only accumulate, and the accrued pension with them.
    #[test]
    fn the_pension_record_only_grows() {
        assert_eq!(run_default(), 0);
        let mut points = 0;
        for index in 0..casivell_project_years() {
            let current = casivell_project_value(index, row::PENSION_POINTS);
            assert!(current >= points);
            points = current;
        }
        assert!(
            points > 20 * 1_000_000,
            "forty years should exceed twenty points"
        );
        assert!(casivell_project_value(0, row::ACCRUED_PENSION) > 0);
    }

    /// A parameter set to something impossible fails, and clears the previous rows rather
    /// than leaving them readable.
    #[test]
    fn a_failed_run_clears_the_previous_rows() {
        assert_eq!(run_default(), 0);
        assert!(casivell_project_years() > 0);

        casivell_project_set(param::YEARS, 500);
        assert_ne!(
            casivell_project_run(),
            0,
            "a 500-year horizon must be refused"
        );
        assert_eq!(casivell_project_years(), 0);
        assert_eq!(
            casivell_project_value(0, row::WEALTH),
            i64::from(error::FIELD)
        );
    }

    /// Unknown parameters and fields are errors rather than silent no-ops.
    #[test]
    fn unknown_parameters_and_fields_are_errors() {
        casivell_project_reset();
        assert_eq!(casivell_project_set(param::COUNT, 1), error::FIELD);
        assert_eq!(casivell_project_set(-1, 1), error::FIELD);

        assert_eq!(run_default(), 0);
        assert_eq!(
            casivell_project_value(0, row::COUNT),
            i64::from(error::FIELD)
        );
        assert_eq!(
            casivell_project_value(999, row::WEALTH),
            i64::from(error::FIELD)
        );
    }

    /// Reset must restore the defaults, so one caller's settings cannot leak into another's
    /// projection — the parameters live in thread-local storage between calls.
    #[test]
    fn reset_restores_the_defaults() {
        casivell_project_reset();
        casivell_project_set(param::YEARS, 5);
        casivell_project_set(param::GROSS, 500_000);
        casivell_project_set(param::EXPENSES, 200_000);
        assert_eq!(casivell_project_run(), 0);
        assert_eq!(casivell_project_years(), 5);

        assert_eq!(run_default(), 0);
        assert_eq!(
            casivell_project_years(),
            40,
            "reset must restore the horizon"
        );
    }

    /// A higher investment return must end richer. The assumption is the browser's main
    /// slider, and a projection that ignored it would look plausible and be useless.
    #[test]
    fn the_return_assumption_moves_the_outcome() {
        let final_wealth = |ppm: i64| {
            casivell_project_reset();
            casivell_project_set(param::GROSS, 500_000);
            casivell_project_set(param::EXPENSES, 200_000);
            casivell_project_set(param::INVESTMENT_RETURN, ppm);
            assert_eq!(casivell_project_run(), 0);
            casivell_project_value(casivell_project_years() - 1, row::WEALTH)
        };
        assert!(
            final_wealth(50_000) > final_wealth(0) * 2,
            "5 % over forty years compounds"
        );
    }
}
