//! Formatting money and rates for display, in German conventions.
//!
//! The engine is `#![no_std]` and has no formatting; this is the first place in the
//! repository that turns a [`Money`] into something a person reads. It stays
//! integer-only — `clippy::float_arithmetic` is denied here as everywhere else, so
//! there is no path by which a display routine could reintroduce the drift the
//! engine was built to avoid.

use casivell_core::{Money, MoneyError, Rate, div_trunc};

/// Formats an amount in German convention: `1.234,56`.
///
/// No currency symbol — callers place it, because column alignment differs between
/// a table and a sentence.
///
/// # Errors
///
/// [`MoneyError`] if the amount cannot be decomposed, which cannot happen for a
/// value inside the domain but is propagated rather than asserted.
pub(crate) fn euro(amount: Money) -> Result<String, MoneyError> {
    let cents = amount.cents();
    let negative = cents < 0;
    let magnitude = cents.checked_abs().ok_or(MoneyError::Overflow)?;

    let whole = div_trunc(magnitude, Money::CENTS_PER_EURO)?;
    let fraction = magnitude
        .checked_rem(Money::CENTS_PER_EURO)
        .ok_or(MoneyError::Overflow)?;

    let sign = if negative { "-" } else { "" };
    Ok(format!("{sign}{}{fraction:02}", thousands(whole)?))
}

/// Inserts `.` as a thousands separator and appends the decimal comma.
///
/// Returns the integer part with its trailing comma, so the caller appends the two
/// fractional digits directly.
fn thousands(whole: i64) -> Result<String, MoneyError> {
    let digits = whole.to_string();
    let mut out = String::with_capacity(digits.len().saturating_add(6));
    let leading = digits.len().checked_rem(3).ok_or(MoneyError::Overflow)?;
    let leading = if leading == 0 { 3 } else { leading };

    for (index, ch) in digits.chars().enumerate() {
        if index >= leading && index.checked_sub(leading).ok_or(MoneyError::Overflow)? % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out.push(',');
    Ok(out)
}

/// Formats a rate as a percentage with two decimals: `14,60 %`.
///
/// # Errors
///
/// [`MoneyError`] on a non-representable intermediate.
pub(crate) fn percent(rate: Rate) -> Result<String, MoneyError> {
    // ppm to hundredths of a percent: 146 000 ppm is 14.60 %, i.e. 1 460 hundredths.
    let hundredths = div_trunc(
        rate.ppm(),
        Rate::PPM_PER_PERCENT_MILLI.checked_mul(10).unwrap_or(100),
    )?;
    let whole = div_trunc(hundredths, 100)?;
    let fraction = hundredths.checked_rem(100).ok_or(MoneyError::Overflow)?;
    Ok(format!("{whole},{:02} %", fraction.unsigned_abs()))
}

/// The share `part` is of `whole`, as a percentage with two decimals.
///
/// Used for the "X % of gross" columns. Returns `None` when `whole` is zero, so the
/// caller renders a dash rather than a division by zero.
///
/// # Errors
///
/// [`MoneyError`] on a non-representable intermediate.
pub(crate) fn share_of(part: Money, whole: Money) -> Result<Option<String>, MoneyError> {
    if whole.is_zero() {
        return Ok(None);
    }
    // Scale to hundredths of a percent before dividing, to keep two decimals.
    let scaled = part
        .cents()
        .checked_mul(10_000)
        .ok_or(MoneyError::Overflow)?;
    let hundredths = div_trunc(scaled, whole.cents())?;
    let integral = div_trunc(hundredths, 100)?;
    let fraction = hundredths
        .checked_rem(100)
        .ok_or(MoneyError::Overflow)?
        .unsigned_abs();
    Ok(Some(format!("{integral},{fraction:02} %")))
}

#[cfg(test)]
mod tests {
    use super::{euro, percent, share_of};
    use casivell_core::{Money, Rate};

    #[test]
    fn amounts_use_german_separators() {
        let cases = [
            (0_i64, "0,00"),
            (5, "0,05"),
            (99, "0,99"),
            (100, "1,00"),
            (123_456, "1.234,56"),
            (100_000_000, "1.000.000,00"),
            (100_000, "1.000,00"),
        ];
        for (cents, expected) in cases {
            let amount = Money::from_cents(cents).expect("in domain");
            assert_eq!(
                euro(amount).expect("formats"),
                expected,
                "for {cents} cents"
            );
        }
    }

    #[test]
    fn negative_amounts_carry_a_leading_sign() {
        let amount = Money::from_cents(-123_456).expect("in domain");
        assert_eq!(euro(amount).expect("formats"), "-1.234,56");
    }

    /// The separator must land every three digits from the right, whatever the
    /// number of leading digits. Off-by-one here is the classic bug.
    ///
    /// Stated in whole euro rather than cents, so the expected grouping is visible in
    /// the input as well as the output.
    #[test]
    fn the_thousands_separator_lands_every_three_digits() {
        let cases = [
            (1_i64, "1,00"),
            (12, "12,00"),
            (123, "123,00"),
            (1_234, "1.234,00"),
            (12_345, "12.345,00"),
            (123_456, "123.456,00"),
            (1_234_567, "1.234.567,00"),
        ];
        for (euros, expected) in cases {
            let amount = Money::from_euro(euros).expect("in domain");
            assert_eq!(euro(amount).expect("formats"), expected, "for {euros} EUR");
        }
    }

    #[test]
    fn rates_render_as_percentages() {
        assert_eq!(
            percent(Rate::from_percent_millis(14_600).expect("valid")).expect("formats"),
            "14,60 %"
        );
        assert_eq!(
            percent(Rate::from_percent_millis(9_300).expect("valid")).expect("formats"),
            "9,30 %"
        );
        assert_eq!(
            percent(Rate::from_percent_millis(600).expect("valid")).expect("formats"),
            "0,60 %"
        );
    }

    #[test]
    fn shares_of_zero_are_reported_as_absent_rather_than_dividing() {
        let part = Money::from_euro(10).expect("valid");
        assert_eq!(share_of(part, Money::ZERO).expect("handles zero"), None);
    }

    #[test]
    fn shares_are_computed_to_two_decimals() {
        let whole = Money::from_euro(4_000).expect("valid");
        let part = Money::from_euro(372).expect("valid");
        // 372 / 4 000 = 9.30 %
        assert_eq!(
            share_of(part, whole).expect("computes"),
            Some("9,30 %".to_owned())
        );
    }
}
