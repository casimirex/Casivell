//! Command-line argument parsing.
//!
//! Hand-rolled rather than using a parser crate. The engine has zero third-party
//! dependencies and that is a property worth defending: every line a reviewer must
//! trust is in this repository. A hundred lines of argument matching is a smaller
//! cost than the audit surface of a dependency tree, for a tool whose entire claim
//! is auditability.

use std::fmt;

use casivell_core::{Money, Rate};
use casivell_lawdata::{Bundesland, TaxClass};
use casivell_payroll::PayPeriod;
use casivell_projection::Assumptions;

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
