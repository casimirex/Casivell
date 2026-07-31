//! Command-line argument parsing.
//!
//! Hand-rolled rather than using a parser crate. The engine has zero third-party
//! dependencies and that is a property worth defending: every line a reviewer must
//! trust is in this repository. A hundred lines of argument matching is a smaller
//! cost than the audit surface of a dependency tree, for a tool whose entire claim
//! is auditability.

use std::fmt;

use casivell_benefits::Variant;
use casivell_core::{Money, Rate};
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::PayPeriod;
use casivell_projection::Assumptions;
use casivell_sim::{Basis, Event, Horizon, Schedule};

/// Anything wrong with the supplied arguments.
#[derive(Debug)]
pub(crate) enum ArgError {
    /// A flag that takes a value was given none.
    MissingValue(String),
    /// A value could not be parsed as the flag's type.
    BadValue {
        /// The flag.
        flag: String,
        /// What was supplied.
        value: String,
        /// What was expected.
        expected: String,
    },
    /// An unrecognised flag.
    Unknown(String),
    /// A required flag was absent.
    Required(String),
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue(flag) => write!(f, "{flag} needs a value"),
            Self::BadValue {
                flag,
                value,
                expected,
            } => write!(f, "{flag}: {value:?} is not {expected}"),
            Self::Unknown(flag) => write!(f, "unknown option {flag}"),
            Self::Required(flag) => write!(f, "{flag} is required"),
        }
    }
}

impl std::error::Error for ArgError {}

/// A parsed request to compute a payslip.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Request {
    /// Gross pay for the period.
    pub(crate) gross: Money,
    /// The pay period the gross figure covers.
    pub(crate) period: PayPeriod,
    /// Tax year.
    pub(crate) year: u16,
    /// Lohnsteuerklasse.
    pub(crate) tax_class: TaxClass,
    /// Federal state, for the church tax rate and the Saxon care split.
    pub(crate) land: Bundesland,
    /// Age in whole years, for the childless care surcharge.
    pub(crate) age: u8,
    /// Children under 25, which reduce the care rate.
    pub(crate) children: u8,
    /// Elterneigenschaft: whether the person has ever had a child.
    pub(crate) is_parent: bool,
    /// Whether church tax is levied.
    pub(crate) church: bool,
    /// The health fund's full supplementary rate.
    pub(crate) supplementary_rate: Rate,
}

/// Defaults chosen to be the commonest case rather than the cheapest to compute.
const DEFAULT_AGE: u8 = 30;
const DEFAULT_YEAR: u16 = 2026;
/// The published average for 2026. A real payslip needs the fund's own rate; this is
/// a starting point, and the report says so.
const DEFAULT_SUPPLEMENTARY_PERCENT_MILLIS: i64 = 2_900;

impl Request {
    /// Parses arguments, excluding the program name.
    ///
    /// # Errors
    ///
    /// [`ArgError`] describing the first problem found.
    pub(crate) fn parse<I>(args: I) -> Result<Self, ArgError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut gross: Option<Money> = None;
        let mut period = PayPeriod::Month;
        let mut year = DEFAULT_YEAR;
        let mut tax_class: Option<TaxClass> = None;
        let mut land = Bundesland::NordrheinWestfalen;
        let mut age = DEFAULT_AGE;
        let mut children = 0_u8;
        let mut is_parent = false;
        let mut church = false;
        let mut supplementary = DEFAULT_SUPPLEMENTARY_PERCENT_MILLIS;

        let mut iter = args.into_iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--gross" | "-g" => gross = Some(parse_money(&flag, &mut iter)?),
                "--class" | "-c" => tax_class = Some(parse_class(&flag, &mut iter)?),
                "--state" | "-s" => land = parse_land(&flag, &mut iter)?,
                "--year" | "-y" => year = parse_u16(&flag, &mut iter)?,
                "--age" => age = parse_u8(&flag, &mut iter)?,
                "--children" => children = parse_u8(&flag, &mut iter)?,
                "--kvz" => supplementary = parse_percent_millis(&flag, &mut iter)?,
                "--period" | "-p" => period = parse_period(&flag, &mut iter)?,
                "--parent" => is_parent = true,
                "--church" => church = true,
                other => return Err(ArgError::Unknown(other.to_owned())),
            }
        }

        // Children under 25 imply Elterneigenschaft; the reverse does not hold, which
        // is why `--parent` exists separately. A parent of grown children pays
        // neither the childless surcharge nor gets a reduction.
        if children > 0 {
            is_parent = true;
        }

        Ok(Self {
            gross: gross.ok_or_else(|| ArgError::Required("--gross".to_owned()))?,
            period,
            year,
            tax_class: tax_class.ok_or_else(|| ArgError::Required("--class".to_owned()))?,
            land,
            age,
            children,
            is_parent,
            church,
            supplementary_rate: Rate::from_percent_millis(supplementary).map_err(|_| {
                ArgError::BadValue {
                    flag: "--kvz".to_owned(),
                    value: supplementary.to_string(),
                    expected: "a plausible percentage".to_owned(),
                }
            })?,
        })
    }
}

fn next_value<I>(flag: &str, iter: &mut I) -> Result<String, ArgError>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| ArgError::MissingValue(flag.to_owned()))
}

/// Parses an amount written as `4500`, `4500.50` or `4500,50`.
fn parse_money<I>(flag: &str, iter: &mut I) -> Result<Money, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "an amount such as 4500 or 4500,50".to_owned(),
    };
    // Accept either decimal separator; strip thousands dots only when a comma is
    // present, so `4.500,00` and `4500.50` both work without ambiguity.
    let normalised = if raw.contains(',') {
        raw.replace('.', "").replace(',', ".")
    } else {
        raw.clone()
    };
    let (whole, fraction) = match normalised.split_once('.') {
        Some((w, f)) => (w, f),
        None => (normalised.as_str(), "0"),
    };
    let euros: i64 = whole.parse().map_err(|_| bad())?;
    // Take at most two decimal places, padding a single digit.
    let cents: u8 = match fraction.len() {
        0 => 0,
        1 => fraction
            .parse::<u8>()
            .map_err(|_| bad())?
            .saturating_mul(10),
        _ => fraction
            .get(..2)
            .ok_or_else(bad)?
            .parse()
            .map_err(|_| bad())?,
    };
    Money::from_euro_cents(euros, cents).map_err(|_| bad())
}

fn parse_class<I>(flag: &str, iter: &mut I) -> Result<TaxClass, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    match raw.to_uppercase().as_str() {
        "1" | "I" => Ok(TaxClass::Class1),
        "2" | "II" => Ok(TaxClass::Class2),
        "3" | "III" => Ok(TaxClass::Class3),
        "4" | "IV" => Ok(TaxClass::Class4),
        "5" | "V" => Ok(TaxClass::Class5),
        "6" | "VI" => Ok(TaxClass::Class6),
        _ => Err(ArgError::BadValue {
            flag: flag.to_owned(),
            value: raw,
            expected: "a tax class 1-6 or I-VI".to_owned(),
        }),
    }
}

fn parse_period<I>(flag: &str, iter: &mut I) -> Result<PayPeriod, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    match raw.to_lowercase().as_str() {
        "month" | "monat" | "m" => Ok(PayPeriod::Month),
        "year" | "jahr" | "y" | "a" => Ok(PayPeriod::Year),
        _ => Err(ArgError::BadValue {
            flag: flag.to_owned(),
            value: raw,
            expected: "`month` or `year` (weekly and daily are not supported)".to_owned(),
        }),
    }
}

fn parse_land<I>(flag: &str, iter: &mut I) -> Result<Bundesland, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    land_from_code(&raw.to_uppercase()).ok_or_else(|| ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw,
        expected: "a state code such as NW, BY or SN".to_owned(),
    })
}

/// Maps the official two-letter state codes.
pub(crate) fn land_from_code(code: &str) -> Option<Bundesland> {
    Some(match code {
        "BW" => Bundesland::BadenWuerttemberg,
        "BY" => Bundesland::Bayern,
        "BE" => Bundesland::Berlin,
        "BB" => Bundesland::Brandenburg,
        "HB" => Bundesland::Bremen,
        "HH" => Bundesland::Hamburg,
        "HE" => Bundesland::Hessen,
        "MV" => Bundesland::MecklenburgVorpommern,
        "NI" => Bundesland::Niedersachsen,
        "NW" => Bundesland::NordrheinWestfalen,
        "RP" => Bundesland::RheinlandPfalz,
        "SL" => Bundesland::Saarland,
        "SN" => Bundesland::Sachsen,
        "ST" => Bundesland::SachsenAnhalt,
        "SH" => Bundesland::SchleswigHolstein,
        "TH" => Bundesland::Thueringen,
        _ => return None,
    })
}

/// The state's code, for display.
#[must_use]
pub(crate) fn land_code(land: Bundesland) -> &'static str {
    match land {
        Bundesland::BadenWuerttemberg => "BW",
        Bundesland::Bayern => "BY",
        Bundesland::Berlin => "BE",
        Bundesland::Brandenburg => "BB",
        Bundesland::Bremen => "HB",
        Bundesland::Hamburg => "HH",
        Bundesland::Hessen => "HE",
        Bundesland::MecklenburgVorpommern => "MV",
        Bundesland::Niedersachsen => "NI",
        Bundesland::NordrheinWestfalen => "NW",
        Bundesland::RheinlandPfalz => "RP",
        Bundesland::Saarland => "SL",
        Bundesland::Sachsen => "SN",
        Bundesland::SachsenAnhalt => "ST",
        Bundesland::SchleswigHolstein => "SH",
        Bundesland::Thueringen => "TH",
    }
}

fn parse_u8<I>(flag: &str, iter: &mut I) -> Result<u8, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    raw.parse().map_err(|_| ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw,
        expected: "a small whole number".to_owned(),
    })
}

fn parse_u16<I>(flag: &str, iter: &mut I) -> Result<u16, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    raw.parse().map_err(|_| ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw,
        expected: "a year such as 2026".to_owned(),
    })
}

/// Parses a percentage such as `2.9` or `2,9` into thousandths of a percent.
fn parse_percent_millis<I>(flag: &str, iter: &mut I) -> Result<i64, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "a percentage such as 2,9".to_owned(),
    };
    let normalised = raw.replace(',', ".");
    let (whole, fraction) = match normalised.split_once('.') {
        Some((w, f)) => (w, f),
        None => (normalised.as_str(), ""),
    };
    let whole: i64 = whole.parse().map_err(|_| bad())?;
    // Pad or truncate the fractional part to exactly three digits.
    let mut thousandths = String::from(fraction);
    thousandths.truncate(3);
    while thousandths.len() < 3 {
        thousandths.push('0');
    }
    let fraction: i64 = thousandths.parse().map_err(|_| bad())?;
    whole
        .checked_mul(1_000)
        .and_then(|w| w.checked_add(fraction))
        .ok_or_else(bad)
}

/// Parses the arguments of the `law` form: a year and the projection assumptions.
///
/// # Errors
///
/// [`ArgError`] describing the first problem found.
pub(crate) fn parse_law<I>(args: I) -> Result<(u16, Assumptions), ArgError>
where
    I: IntoIterator<Item = String>,
{
    let mut year: Option<u16> = None;
    let mut inflation = Assumptions::DEFAULT_PRICE_INFLATION_PERCENT_MILLIS;
    let mut wages = Assumptions::DEFAULT_WAGE_GROWTH_PERCENT_MILLIS;

    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--year" | "-y" => year = Some(parse_u16(&flag, &mut iter)?),
            "--inflation" => inflation = parse_percent_millis(&flag, &mut iter)?,
            "--wage-growth" => wages = parse_percent_millis(&flag, &mut iter)?,
            other => return Err(ArgError::Unknown(other.to_owned())),
        }
    }

    let assumptions =
        Assumptions::from_percent_millis(inflation, wages).map_err(|_| ArgError::BadValue {
            flag: "--inflation/--wage-growth".to_owned(),
            value: "out of range".to_owned(),
            expected: "an annual rate within ±20 %".to_owned(),
        })?;

    Ok((
        year.ok_or_else(|| ArgError::Required("--year".to_owned()))?,
        assumptions,
    ))
}

/// A parsed request to project a household forward.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectRequest {
    /// The household's employment and circumstances, shared with the payslip form so the
    /// two cannot disagree about the same person.
    pub(crate) base: Request,
    /// How long to project.
    pub(crate) horizon: Horizon,
    /// Nominal or real.
    pub(crate) basis: Basis,
    /// Statutory growth assumptions.
    pub(crate) assumptions: Assumptions,
    /// Annual nominal investment return on accumulated wealth.
    pub(crate) investment_return: Rate,
    /// Annual nominal growth in the value of owned property.
    pub(crate) property_growth: Rate,
    /// Monthly expenses.
    pub(crate) monthly_expenses: Money,
    /// Annual growth in the household's own pay.
    pub(crate) pay_growth: Rate,
    /// Life events departing from the baseline.
    pub(crate) schedule: Schedule,
}

/// Parses the arguments of the `project` form.
///
/// # Errors
///
/// [`ArgError`] describing the first problem found.
pub(crate) fn parse_project<I>(args: I) -> Result<ProjectRequest, ArgError>
where
    I: IntoIterator<Item = String>,
{
    // Collected first so the shared household flags can be handed to `Request::parse`
    // unchanged, rather than duplicating their parsing here.
    let mut shared: Vec<String> = Vec::new();
    let mut years = 40_u32;
    let mut basis = Basis::Nominal;
    let mut inflation = Assumptions::DEFAULT_PRICE_INFLATION_PERCENT_MILLIS;
    let mut wages = Assumptions::DEFAULT_WAGE_GROWTH_PERCENT_MILLIS;
    let mut investment = 0_i64;
    let mut property_growth = 0_i64;
    let mut expenses: Option<Money> = None;
    let mut pay_growth = 0_i64;
    let mut schedule = Schedule::new();

    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--years" => years = u32::from(parse_u16(&flag, &mut iter)?),
            "--real" => basis = Basis::Real,
            "--inflation" => inflation = parse_percent_millis(&flag, &mut iter)?,
            "--wage-growth" => wages = parse_percent_millis(&flag, &mut iter)?,
            "--return" => investment = parse_percent_millis(&flag, &mut iter)?,
            "--property-growth" => property_growth = parse_percent_millis(&flag, &mut iter)?,
            "--pay-growth" => pay_growth = parse_percent_millis(&flag, &mut iter)?,
            "--expenses" => expenses = Some(parse_money(&flag, &mut iter)?),
            "--part-time"
            | "--break"
            | "--raise"
            | "--one-off"
            | "--parental-leave"
            | "--parental-leave-plus"
            | "--child-born"
            | "--buy" => {
                parse_event_flag(&flag, &mut iter, &mut schedule)?;
            }
            other => {
                // Anything else belongs to the shared household description. Value-taking
                // flags carry their value with them.
                shared.push(other.to_owned());
                if takes_a_value(other) {
                    shared.push(next_value(other, &mut iter)?);
                }
            }
        }
    }

    let bad = |flag: &str, expected: &str| ArgError::BadValue {
        flag: flag.to_owned(),
        value: "out of range".to_owned(),
        expected: expected.to_owned(),
    };

    Ok(ProjectRequest {
        base: Request::parse(shared)?,
        horizon: Horizon::years(years).map_err(|_| bad("--years", "a horizon up to 70 years"))?,
        basis,
        assumptions: Assumptions::from_percent_millis(inflation, wages)
            .map_err(|_| bad("--inflation/--wage-growth", "an annual rate within ±20 %"))?,
        investment_return: Rate::from_percent_millis(investment)
            .map_err(|_| bad("--return", "an annual rate within ±1000 %"))?,
        property_growth: Rate::from_percent_millis(property_growth)
            .map_err(|_| bad("--property-growth", "an annual rate within ±1000 %"))?,
        monthly_expenses: expenses.ok_or_else(|| ArgError::Required("--expenses".to_owned()))?,
        pay_growth: Rate::from_percent_millis(pay_growth)
            .map_err(|_| bad("--pay-growth", "an annual rate within ±1000 %"))?,
        schedule,
    })
}

/// Parses one life-event flag and adds it to the schedule.
///
/// Split out from [`parse_project`] to keep both inside the sixty-line limit, and because the
/// event grammar is a self-contained thing worth reading on its own.
fn parse_event_flag<I>(flag: &str, iter: &mut I, schedule: &mut Schedule) -> Result<(), ArgError>
where
    I: Iterator<Item = String>,
{
    let event = match flag {
        "--part-time" => {
            let spec = parse_period_spec(flag, iter)?;
            Event::WorkingTime {
                from_month: spec.from_month,
                until_month: spec.until_month,
                fraction: Rate::from_percent_millis(spec.value).map_err(|_| {
                    ArgError::BadValue {
                        flag: flag.to_owned(),
                        value: spec.value.to_string(),
                        expected: "a share of full time, such as 60".to_owned(),
                    }
                })?,
            }
        }
        "--break" => {
            let (from_month, until) = parse_window(flag, iter)?;
            Event::UnpaidLeave {
                from_month,
                until_month: Some(until),
            }
        }
        "--raise" => {
            let (from_month, monthly_gross) = parse_month_amount(flag, iter)?;
            Event::PayChange {
                from_month,
                monthly_gross,
            }
        }
        "--buy" => purchase_event(flag, iter)?,
        "--child-born" => birth_event(flag, iter)?,
        "--parental-leave" | "--parental-leave-plus" => leave_event(flag, iter)?,
        _ => {
            let (month, amount) = parse_month_amount(flag, iter)?;
            Event::OneOff { month, amount }
        }
    };
    *schedule = schedule.with(event).map_err(|_| schedule_error(flag))?;
    Ok(())
}

/// The error a schedule refuses an event with.
///
/// The two ways it can refuse are the bound on the event count and a window that ends before
/// it starts, and neither is worth distinguishing to a user who has mistyped one flag.
fn schedule_error(flag: &str) -> ArgError {
    ArgError::BadValue {
        flag: flag.to_owned(),
        value: "too many events, or a window ending before it starts".to_owned(),
        expected: "at most 32 events, each ending after it starts".to_owned(),
    }
}

/// Builds a parental-leave event from its flag.
///
/// `FROM:MONTHS[:PERCENT]` — months rather than years, because parental leave is counted in
/// Lebensmonate and a household says "fourteen months", never "1.17 years". The optional
/// percent is part-time work during the leave.
fn leave_event<I>(flag: &str, iter: &mut I) -> Result<Event, ArgError>
where
    I: Iterator<Item = String>,
{
    let spec = parse_leave_spec(flag, iter)?;
    Ok(Event::ParentalLeave {
        from_month: spec.from_month,
        months: spec.months,
        working_fraction: spec.working_fraction,
        variant: if flag == "--parental-leave-plus" {
            Variant::Plus
        } else {
            Variant::Basis
        },
        sibling_bonus: false,
        additional_children: 0,
    })
}

/// Builds a birth event from its flag.
fn birth_event<I>(flag: &str, iter: &mut I) -> Result<Event, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let month = years_to_months(&raw).ok_or_else(|| ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "a year offset into the projection, e.g. 2".to_owned(),
    })?;
    Ok(Event::ChildBorn { month })
}

/// Builds a purchase event from its flag.
///
/// The mortgage terms are not on the flag: a projection's point is the household's position
/// over decades, and `casivell property` is the form for pricing a particular offer. The
/// defaults are the conventional 3,5 % and 2 %.
fn purchase_event<I>(flag: &str, iter: &mut I) -> Result<Event, ArgError>
where
    I: Iterator<Item = String>,
{
    let spec = parse_purchase(flag, iter)?;
    Ok(Event::PropertyPurchase {
        month: spec.month,
        price: spec.price,
        land: spec.land,
        deposit: spec.deposit,
        agent_rate: Rate::ZERO,
        interest_rate: Rate::from_percent_millis(3_500).unwrap_or(Rate::ZERO),
        repayment_rate: Rate::from_percent_millis(2_000).unwrap_or(Rate::ZERO),
        monthly_expenses_after: spec.monthly_expenses_after,
    })
}

/// A property purchase specification.
struct PurchaseSpec {
    month: u32,
    price: Money,
    deposit: Money,
    land: Bundesland,
    monthly_expenses_after: Money,
}

/// Parses `YEAR:PRICE:DEPOSIT:STATE[:EXPENSES]`.
///
/// `EXPENSES` is the household's monthly outgoing afterwards excluding the mortgage; omitted,
/// it keeps whatever `--expenses` said, which is almost never right once the rent has stopped
/// and is therefore documented in the report rather than silently assumed to be.
fn parse_purchase<I>(flag: &str, iter: &mut I) -> Result<PurchaseSpec, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "YEAR:PRICE:DEPOSIT:STATE[:EXPENSES], e.g. 3:400000:100000:NW:900".to_owned(),
    };
    // Destructured rather than indexed, so the arity check and the field assignment are the
    // same statement and cannot disagree.
    let parts: Vec<&str> = raw.split(':').collect();
    let (year, price, deposit, state, expenses) = match parts.as_slice() {
        [year, price, deposit, state] => (*year, *price, *deposit, *state, None),
        [year, price, deposit, state, expenses] => {
            (*year, *price, *deposit, *state, Some(*expenses))
        }
        _ => return Err(bad()),
    };

    let money = |text: &str| -> Result<Money, ArgError> {
        let euros: i64 = text.parse().map_err(|_| bad())?;
        Money::from_euro(euros).map_err(|_| bad())
    };

    Ok(PurchaseSpec {
        month: years_to_months(year).ok_or_else(bad)?,
        price: money(price)?,
        deposit: money(deposit)?,
        land: land_from_code(&state.to_uppercase()).ok_or_else(bad)?,
        monthly_expenses_after: match expenses {
            Some(text) => money(text)?,
            None => Money::ZERO,
        },
    })
}

/// A parental leave specification.
struct LeaveSpec {
    from_month: u32,
    months: u32,
    working_fraction: Rate,
}

/// Parses `FROM:MONTHS[:PERCENT]`, where `FROM` is a year offset and `MONTHS` a count of
/// Lebensmonate. `2:14` is "fourteen months from the start of year 2"; `2:14:50` adds
/// half-time work throughout.
fn parse_leave_spec<I>(flag: &str, iter: &mut I) -> Result<LeaveSpec, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "FROM:MONTHS[:PERCENT], e.g. 2:14 or 2:24:50".to_owned(),
    };
    let mut parts = raw.split(':');
    let from = parts.next().ok_or_else(bad)?;
    let months = parts.next().ok_or_else(bad)?;
    let percent = parts.next();
    if parts.next().is_some() {
        return Err(bad());
    }

    let working_fraction = match percent {
        None => Rate::ZERO,
        Some(text) => {
            let value: i64 = text.parse().map_err(|_| bad())?;
            Rate::from_percent_millis(value.checked_mul(1_000).ok_or_else(bad)?)
                .map_err(|_| bad())?
        }
    };

    Ok(LeaveSpec {
        from_month: years_to_months(from).ok_or_else(bad)?,
        months: months.parse().map_err(|_| bad())?,
        working_fraction,
    })
}

/// A request to price a property purchase.
pub(crate) struct PropertyRequest {
    /// The agreed price.
    pub(crate) price: Money,
    /// Where the property is, which decides the Grunderwerbsteuer.
    pub(crate) land: Bundesland,
    /// The buyer's own money.
    pub(crate) deposit: Money,
    /// The buyer's share of any Maklerprovision.
    pub(crate) agent_rate: Rate,
    /// The Sollzins.
    pub(crate) interest_rate: Rate,
    /// The anfängliche Tilgung.
    pub(crate) repayment_rate: Rate,
    /// The Zinsbindung in years.
    pub(crate) fixed_years: u32,
    /// The year, for the statutory rates.
    pub(crate) year: u16,
}

/// Parses a percentage into a [`Rate`], refusing one outside `0..=100`.
fn parse_rate<I>(flag: &str, iter: &mut I) -> Result<Rate, ArgError>
where
    I: Iterator<Item = String>,
{
    let millis = parse_percent_millis(flag, iter)?;
    Rate::from_percent_millis(millis).map_err(|_| ArgError::BadValue {
        flag: flag.to_owned(),
        value: millis.to_string(),
        expected: "a percentage between 0 and 100".to_owned(),
    })
}

/// Parses the `property` form.
///
/// # Errors
///
/// [`ArgError`] on an unusable argument, including a missing `--price`.
pub(crate) fn parse_property(args: Vec<String>) -> Result<PropertyRequest, ArgError> {
    let mut price = None;
    let mut land = Bundesland::NordrheinWestfalen;
    let mut deposit = Money::ZERO;
    let mut agent = Rate::ZERO;
    let mut interest = Rate::from_percent_millis(3_500).unwrap_or(Rate::ZERO);
    let mut repayment = Rate::from_percent_millis(2_000).unwrap_or(Rate::ZERO);
    let mut fixed_years = 10_u32;
    let mut year = DEFAULT_YEAR;

    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--price" => price = Some(parse_money(&flag, &mut iter)?),
            "--state" => land = parse_land(&flag, &mut iter)?,
            "--deposit" => deposit = parse_money(&flag, &mut iter)?,
            "--agent" => agent = parse_rate(&flag, &mut iter)?,
            "--rate" => interest = parse_rate(&flag, &mut iter)?,
            "--tilgung" => repayment = parse_rate(&flag, &mut iter)?,
            "--fixed" => fixed_years = u32::from(parse_u16(&flag, &mut iter)?),
            "--year" => year = parse_u16(&flag, &mut iter)?,
            other => return Err(ArgError::Unknown(other.to_owned())),
        }
    }

    Ok(PropertyRequest {
        price: price.ok_or_else(|| ArgError::Required("--price".to_owned()))?,
        land,
        deposit,
        agent_rate: agent,
        interest_rate: interest,
        repayment_rate: repayment,
        fixed_years,
        year,
    })
}

/// A request for an annual assessment.
pub(crate) struct AssessRequest {
    /// The household description.
    pub(crate) base: Request,
    /// Actual Werbungskosten, where they exceed the § 9a Pauschbetrag.
    pub(crate) work_expenses: Money,
    /// Other Sonderausgaben under §§ 10–10b: donations, maintenance, training.
    pub(crate) other_special_expenses: Money,
    /// Gross capital income for the year, § 20 EStG.
    pub(crate) capital_income: Money,
    /// Tax-free wage-replacement benefits received, § 32b Abs. 1 Nr. 1.
    pub(crate) benefits: Money,
    /// The §§ 33 and 33b claim.
    pub(crate) extraordinary: casivell_income::BurdenClaim,
}

/// Parses the `assess` form.
///
/// # Errors
///
/// [`ArgError`] on an unusable argument.
pub(crate) fn parse_assess(args: Vec<String>) -> Result<AssessRequest, ArgError> {
    let mut shared = Vec::with_capacity(args.len());
    let (mut work_expenses, mut other, mut capital, mut benefits) =
        (Money::ZERO, Money::ZERO, Money::ZERO, Money::ZERO);
    let mut extraordinary = casivell_income::BurdenClaim::default();

    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--work-expenses" => work_expenses = parse_money(&flag, &mut iter)?,
            "--donations" => other = parse_money(&flag, &mut iter)?,
            "--capital" => capital = parse_money(&flag, &mut iter)?,
            "--benefits" => benefits = parse_money(&flag, &mut iter)?,
            "--medical" => extraordinary.general_costs = parse_money(&flag, &mut iter)?,
            "--disability" => {
                extraordinary.disability_degree = parse_small(
                    &flag,
                    &mut iter,
                    100,
                    "a Grad der Behinderung from 0 to 100",
                )?;
            }
            "--helpless" => extraordinary.helpless = true,
            "--care-grade" => {
                extraordinary.care_grade =
                    parse_small(&flag, &mut iter, 5, "a Pflegegrad from 0 to 5")?;
            }
            other_flag => {
                shared.push(other_flag.to_owned());
                if takes_a_value(other_flag) {
                    shared.push(next_value(other_flag, &mut iter)?);
                }
            }
        }
    }

    Ok(AssessRequest {
        base: Request::parse(shared)?,
        work_expenses,
        other_special_expenses: other,
        capital_income: capital,
        benefits,
        extraordinary,
    })
}

/// Parses a small bounded integer, refusing anything past `maximum`.
fn parse_small<I>(flag: &str, iter: &mut I, maximum: u8, expected: &str) -> Result<u8, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: expected.to_owned(),
    };
    let value: u8 = raw.parse().map_err(|_| bad())?;
    if value > maximum {
        return Err(bad());
    }
    Ok(value)
}

/// A request to compare tax-class arrangements.
pub(crate) struct ClassesRequest {
    /// The household description, carrying the first salary.
    pub(crate) base: Request,
    /// The second spouse's monthly gross.
    pub(crate) partner_gross: Money,
}

/// Parses the `classes` form.
///
/// # Errors
///
/// [`ArgError`] on an unusable argument, including a missing `--partner`.
pub(crate) fn parse_classes(args: Vec<String>) -> Result<ClassesRequest, ArgError> {
    let mut shared = Vec::with_capacity(args.len());
    let mut partner_gross = None;

    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--partner" => partner_gross = Some(parse_money(&flag, &mut iter)?),
            other => {
                shared.push(other.to_owned());
                if takes_a_value(other) {
                    shared.push(next_value(other, &mut iter)?);
                }
            }
        }
    }

    Ok(ClassesRequest {
        base: Request::parse(shared)?,
        partner_gross: partner_gross.ok_or_else(|| ArgError::Required("--partner".to_owned()))?,
    })
}

/// A windowed event specification, `FROM:UNTIL:VALUE` in years.
struct PeriodSpec {
    from_month: u32,
    until_month: Option<u32>,
    value: i64,
}

/// Parses `FROM:UNTIL:VALUE`, where the years are offsets into the projection and `UNTIL` may
/// be empty for open-ended. `3:8:60` is "from year 3 to year 8, at 60 %".
fn parse_period_spec<I>(flag: &str, iter: &mut I) -> Result<PeriodSpec, ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "FROM:UNTIL:PERCENT in years, e.g. 3:8:60 (UNTIL may be empty)".to_owned(),
    };
    let mut parts = raw.split(':');
    let from = parts.next().ok_or_else(bad)?;
    let until = parts.next().ok_or_else(bad)?;
    let value = parts.next().ok_or_else(bad)?;
    if parts.next().is_some() {
        return Err(bad());
    }
    Ok(PeriodSpec {
        from_month: years_to_months(from).ok_or_else(bad)?,
        until_month: if until.is_empty() {
            None
        } else {
            Some(years_to_months(until).ok_or_else(bad)?)
        },
        value: parse_percent_millis_str(value).ok_or_else(bad)?,
    })
}

/// Parses `FROM:UNTIL` in years.
fn parse_window<I>(flag: &str, iter: &mut I) -> Result<(u32, u32), ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "FROM:UNTIL in years, e.g. 5:6".to_owned(),
    };
    let (from, until) = raw.split_once(':').ok_or_else(bad)?;
    Ok((
        years_to_months(from).ok_or_else(bad)?,
        years_to_months(until).ok_or_else(bad)?,
    ))
}

/// Parses `YEAR:AMOUNT`.
fn parse_month_amount<I>(flag: &str, iter: &mut I) -> Result<(u32, Money), ArgError>
where
    I: Iterator<Item = String>,
{
    let raw = next_value(flag, iter)?;
    let bad = || ArgError::BadValue {
        flag: flag.to_owned(),
        value: raw.clone(),
        expected: "YEAR:AMOUNT, e.g. 15:8000 or 5:-60000".to_owned(),
    };
    let (year, amount) = raw.split_once(':').ok_or_else(bad)?;
    let month = years_to_months(year).ok_or_else(bad)?;
    Ok((month, parse_money_str(amount).ok_or_else(bad)?))
}

/// Whole years as a month offset. Fractional years are deliberately not accepted: a life event
/// stated to the month is a precision the rest of the input does not have.
fn years_to_months(text: &str) -> Option<u32> {
    text.parse::<u32>().ok()?.checked_mul(12)
}

/// A percentage in thousandths, from a bare string.
fn parse_percent_millis_str(text: &str) -> Option<i64> {
    let normalised = text.replace(',', ".");
    let (whole, fraction) = normalised
        .split_once('.')
        .unwrap_or((normalised.as_str(), ""));
    let whole: i64 = whole.parse().ok()?;
    let mut thousandths = String::from(fraction);
    thousandths.truncate(3);
    while thousandths.len() < 3 {
        thousandths.push('0');
    }
    whole
        .checked_mul(1_000)?
        .checked_add(thousandths.parse::<i64>().ok()?)
}

/// An amount from a bare string, accepting a leading minus.
fn parse_money_str(text: &str) -> Option<Money> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let euros: i64 = digits.replace('.', "").parse().ok()?;
    let signed = if negative {
        euros.checked_neg()?
    } else {
        euros
    };
    Money::from_euro(signed).ok()
}

/// Whether a flag of the payslip form consumes the following argument.
fn takes_a_value(flag: &str) -> bool {
    matches!(
        flag,
        "--gross"
            | "-g"
            | "--class"
            | "-c"
            | "--state"
            | "-s"
            | "--year"
            | "-y"
            | "--age"
            | "--children"
            | "--kvz"
            | "--period"
            | "-p"
    )
}

#[cfg(test)]
mod tests {
    use super::{ArgError, Request, land_code, land_from_code, parse_law};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_payroll::PayPeriod;

    fn parse(args: &[&str]) -> Result<Request, ArgError> {
        Request::parse(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn a_minimal_invocation_parses_with_sensible_defaults() {
        let r = parse(&["--gross", "4500", "--class", "1"]).expect("parses");
        assert_eq!(r.gross.cents(), 450_000);
        assert_eq!(r.tax_class, TaxClass::Class1);
        assert_eq!(r.period, PayPeriod::Month);
        assert_eq!(r.year, 2026);
        assert!(!r.church);
        assert!(!r.is_parent);
    }

    #[test]
    fn both_decimal_conventions_are_accepted() {
        for text in ["4500,50", "4500.50", "4.500,50"] {
            let r = parse(&["--gross", text, "-c", "1"]).expect("parses");
            assert_eq!(r.gross.cents(), 450_050, "failed on {text}");
        }
    }

    #[test]
    fn a_single_decimal_digit_is_padded() {
        let r = parse(&["--gross", "100,5", "-c", "1"]).expect("parses");
        assert_eq!(r.gross.cents(), 10_050);
    }

    #[test]
    fn tax_classes_accept_arabic_and_roman_numerals() {
        for (text, expected) in [
            ("1", TaxClass::Class1),
            ("I", TaxClass::Class1),
            ("iii", TaxClass::Class3),
            ("6", TaxClass::Class6),
            ("VI", TaxClass::Class6),
        ] {
            let r = parse(&["--gross", "1", "-c", text]).expect("parses");
            assert_eq!(r.tax_class, expected, "failed on {text}");
        }
    }

    /// Children under 25 imply Elterneigenschaft, because the reverse would levy the
    /// childless surcharge on someone with young children.
    #[test]
    fn children_imply_parenthood() {
        let r = parse(&["--gross", "1", "-c", "1", "--children", "2"]).expect("parses");
        assert!(r.is_parent);
        assert_eq!(r.children, 2);

        // But parenthood alone does not imply children under 25.
        let r = parse(&["--gross", "1", "-c", "1", "--parent"]).expect("parses");
        assert!(r.is_parent);
        assert_eq!(r.children, 0);
    }

    #[test]
    fn the_supplementary_rate_parses_to_thousandths_of_a_percent() {
        let r = parse(&["--gross", "1", "-c", "1", "--kvz", "1,7"]).expect("parses");
        assert_eq!(r.supplementary_rate.ppm(), 17_000);
        let r = parse(&["--gross", "1", "-c", "1", "--kvz", "2.90"]).expect("parses");
        assert_eq!(r.supplementary_rate.ppm(), 29_000);
    }

    #[test]
    fn missing_required_flags_are_reported() {
        assert!(matches!(
            parse(&["--class", "1"]),
            Err(ArgError::Required(_))
        ));
        assert!(matches!(
            parse(&["--gross", "1000"]),
            Err(ArgError::Required(_))
        ));
    }

    #[test]
    fn bad_and_unknown_flags_are_reported_rather_than_ignored() {
        assert!(matches!(
            parse(&["--gross", "1", "-c", "9"]),
            Err(ArgError::BadValue { .. })
        ));
        assert!(matches!(
            parse(&["--gross", "1", "-c", "1", "--nonsense"]),
            Err(ArgError::Unknown(_))
        ));
        assert!(matches!(
            parse(&["--gross"]),
            Err(ArgError::MissingValue(_))
        ));
    }

    /// Weekly and daily periods are unsupported, and must be refused with an
    /// explanation rather than silently treated as monthly.
    #[test]
    fn unsupported_pay_periods_are_refused() {
        let err = parse(&["--gross", "1", "-c", "1", "-p", "week"]).expect_err("refuses");
        let message = err.to_string();
        assert!(message.contains("weekly and daily"), "unhelpful: {message}");
    }

    /// The code mapping must round-trip for all sixteen states, or a state would be
    /// unselectable or display wrongly.
    #[test]
    fn every_state_code_round_trips() {
        for land in Bundesland::ALL {
            let code = land_code(land);
            assert_eq!(
                land_from_code(code),
                Some(land),
                "{land:?} does not round-trip through {code}"
            );
        }
    }

    #[test]
    fn state_codes_are_case_insensitive() {
        let r = parse(&["--gross", "1", "-c", "1", "-s", "sn"]).expect("parses");
        assert_eq!(r.land, Bundesland::Sachsen);
    }

    /// The law form needs a year and defaults its assumptions to the documented ones.
    #[test]
    fn the_law_form_defaults_its_assumptions() {
        let (year, assumptions) =
            parse_law(["--year".to_owned(), "2040".to_owned()]).expect("parses");
        assert_eq!(year, 2040);
        assert_eq!(assumptions, casivell_projection::Assumptions::default());
    }

    #[test]
    fn the_law_form_accepts_explicit_assumptions() {
        let (_, assumptions) = parse_law(
            [
                "--year",
                "2040",
                "--inflation",
                "3,5",
                "--wage-growth",
                "4,0",
            ]
            .iter()
            .map(|s| (*s).to_owned()),
        )
        .expect("parses");
        assert_eq!(assumptions.price_inflation().ppm(), 35_000);
        assert_eq!(assumptions.wage_growth().ppm(), 40_000);
    }

    #[test]
    fn the_law_form_reports_a_missing_year_and_unknown_flags() {
        assert!(matches!(
            parse_law(core::iter::empty()),
            Err(ArgError::Required(_))
        ));
        assert!(matches!(
            parse_law(["--nonsense".to_owned()]),
            Err(ArgError::Unknown(_))
        ));
    }

    /// An implausible assumption must be refused rather than clamped.
    #[test]
    fn the_law_form_refuses_an_implausible_assumption() {
        assert!(matches!(
            parse_law(
                ["--year", "2040", "--inflation", "500,0"]
                    .iter()
                    .map(|s| (*s).to_owned())
            ),
            Err(ArgError::BadValue { .. })
        ));
    }
}
