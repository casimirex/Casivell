//! Integer division with an explicit, named rounding direction.
//!
//! Rust's `/` truncates toward zero. German statute variously requires
//! truncation toward zero, flooring toward negative infinity, or commercial
//! half-up rounding, and the three disagree for negative operands. Rather than
//! let the reader guess which one a bare `/` meant, the engine never uses `/` on
//! monetary quantities: it calls one of these functions by name and cites the
//! paragraph that demands it.
//!
//! Every function here is total. `checked_div` and `checked_rem` are used
//! throughout rather than `/` and `%` so that a zero divisor or the single
//! non-representable two's-complement division becomes a value, not a panic.

use crate::money::MoneyError;

/// Which way to break a division that does not divide evenly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Toward negative infinity. `-7 / 2 == -4`.
    ///
    /// This is *abrunden* in the statutory sense, and is what
    /// § 32a Abs. 1 EStG means by "auf einen volle Euro-Betrag abgerundet".
    Floor,
    /// Toward zero. `-7 / 2 == -3`. Rust's native `/`.
    TowardZero,
    /// Toward positive infinity. `-7 / 2 == -3`, `7 / 2 == 4`.
    ///
    /// This is *aufrunden*. The BMF Programmablaufplan needs it: the
    /// Vorsorgepauschale boxes `VSP = VSPKVPV + VSPR` and `VSPN = VSPR + VSPHB`
    /// are annotated `Euro↑`, while every other `Euro` annotation in the same
    /// document points down. Rounding those two the wrong way puts the annual
    /// Lohnsteuer out by a euro or two across most of the income range.
    Ceiling,
    /// Nearest, ties away from zero. `-7 / 2 == -4`, `7 / 2 == 4`.
    ///
    /// Commercial rounding, used where a statute says "kaufmännisch gerundet".
    HalfUp,
}

/// Divides `n` by `d`, rounding toward negative infinity.
///
/// # Errors
///
/// [`MoneyError::DivisionByZero`] when `d == 0`; [`MoneyError::Overflow`] for
/// `i64::MIN / -1`, the one division whose true result is not representable.
pub const fn div_floor(n: i64, d: i64) -> Result<i64, MoneyError> {
    let (q, r) = match quotient_and_remainder(n, d) {
        Ok(parts) => parts,
        Err(e) => return Err(e),
    };
    // A non-zero remainder whose sign opposes the divisor means truncation
    // rounded up; step down one to reach the floor.
    if r != 0 && ((r < 0) != (d < 0)) {
        return match q.checked_sub(1) {
            Some(v) => Ok(v),
            None => Err(MoneyError::Overflow),
        };
    }
    Ok(q)
}

/// Divides `n` by `d`, rounding toward positive infinity.
///
/// # Errors
///
/// As [`div_floor`].
pub const fn div_ceil(n: i64, d: i64) -> Result<i64, MoneyError> {
    let (q, r) = match quotient_and_remainder(n, d) {
        Ok(parts) => parts,
        Err(e) => return Err(e),
    };
    // A non-zero remainder sharing the divisor's sign means the exact quotient lies
    // above the truncated one; step up to reach the ceiling.
    if r != 0 && ((r < 0) == (d < 0)) {
        return match q.checked_add(1) {
            Some(v) => Ok(v),
            None => Err(MoneyError::Overflow),
        };
    }
    Ok(q)
}

/// Divides `n` by `d`, rounding toward zero.
///
/// # Errors
///
/// As [`div_floor`].
pub const fn div_trunc(n: i64, d: i64) -> Result<i64, MoneyError> {
    match quotient_and_remainder(n, d) {
        Ok((q, _)) => Ok(q),
        Err(e) => Err(e),
    }
}

/// Divides `n` by `d`, rounding to nearest with ties away from zero.
///
/// # Errors
///
/// As [`div_floor`].
pub const fn div_round_half_up(n: i64, d: i64) -> Result<i64, MoneyError> {
    let (q, r) = match quotient_and_remainder(n, d) {
        Ok(parts) => parts,
        Err(e) => return Err(e),
    };
    if r == 0 {
        return Ok(q);
    }
    // Decide the tie by comparing 2*|r| with |d|, staying in integers.
    // |r| < |d| <= i64::MAX always holds, so the doubling cannot overflow.
    let twice_r = match r.checked_mul(2) {
        Some(v) => v.unsigned_abs(),
        None => return Err(MoneyError::Overflow),
    };
    if twice_r < d.unsigned_abs() {
        return Ok(q);
    }
    // Away from zero: the exact quotient's sign is the sign of n XOR the sign of d.
    let step = if (n < 0) == (d < 0) { 1 } else { -1 };
    match q.checked_add(step) {
        Some(v) => Ok(v),
        None => Err(MoneyError::Overflow),
    }
}

/// Divides `n` by `d` using the named `mode`.
///
/// # Errors
///
/// As [`div_floor`].
pub const fn div(n: i64, d: i64, mode: Rounding) -> Result<i64, MoneyError> {
    match mode {
        Rounding::Floor => div_floor(n, d),
        Rounding::Ceiling => div_ceil(n, d),
        Rounding::TowardZero => div_trunc(n, d),
        Rounding::HalfUp => div_round_half_up(n, d),
    }
}

/// The single shared precondition check, so the public entry points cannot drift
/// apart on how they reject a bad divisor.
const fn quotient_and_remainder(n: i64, d: i64) -> Result<(i64, i64), MoneyError> {
    if d == 0 {
        return Err(MoneyError::DivisionByZero);
    }
    match (n.checked_div(d), n.checked_rem(d)) {
        (Some(q), Some(r)) => Ok((q, r)),
        _ => Err(MoneyError::Overflow),
    }
}

#[cfg(test)]
mod tests {
    use super::{Rounding, div, div_ceil, div_floor, div_round_half_up, div_trunc};
    use crate::money::MoneyError;

    #[test]
    fn floor_rounds_toward_negative_infinity() {
        assert_eq!(div_floor(7, 2), Ok(3));
        assert_eq!(div_floor(-7, 2), Ok(-4));
        assert_eq!(div_floor(7, -2), Ok(-4));
        assert_eq!(div_floor(-7, -2), Ok(3));
        assert_eq!(div_floor(8, 2), Ok(4));
        assert_eq!(div_floor(-8, 2), Ok(-4));
    }

    #[test]
    fn trunc_rounds_toward_zero() {
        assert_eq!(div_trunc(7, 2), Ok(3));
        assert_eq!(div_trunc(-7, 2), Ok(-3));
        assert_eq!(div_trunc(7, -2), Ok(-3));
    }

    #[test]
    fn half_up_breaks_ties_away_from_zero() {
        assert_eq!(div_round_half_up(5, 2), Ok(3));
        assert_eq!(div_round_half_up(-5, 2), Ok(-3));
        assert_eq!(div_round_half_up(4, 3), Ok(1));
        assert_eq!(div_round_half_up(5, 3), Ok(2));
        assert_eq!(div_round_half_up(-5, 3), Ok(-2));
    }

    #[test]
    fn rejects_zero_divisor_rather_than_panicking() {
        assert_eq!(div_floor(1, 0), Err(MoneyError::DivisionByZero));
        assert_eq!(div_trunc(1, 0), Err(MoneyError::DivisionByZero));
        assert_eq!(div_round_half_up(1, 0), Err(MoneyError::DivisionByZero));
    }

    #[test]
    fn rejects_the_one_overflowing_division() {
        assert_eq!(div_floor(i64::MIN, -1), Err(MoneyError::Overflow));
        assert_eq!(div_trunc(i64::MIN, -1), Err(MoneyError::Overflow));
    }

    #[test]
    fn ceiling_rounds_toward_positive_infinity() {
        assert_eq!(div_ceil(7, 2), Ok(4));
        assert_eq!(div_ceil(-7, 2), Ok(-3));
        assert_eq!(div_ceil(7, -2), Ok(-3));
        assert_eq!(div_ceil(-7, -2), Ok(4));
        assert_eq!(div_ceil(8, 2), Ok(4));
        assert_eq!(div_ceil(-8, 2), Ok(-4));
    }

    /// Flooring and ceiling must bracket the exact quotient and differ by exactly
    /// one whenever the division is inexact. This is the property the PAP relies on
    /// when it uses both directions in the same algorithm.
    #[test]
    fn floor_and_ceiling_bracket_the_exact_quotient() {
        for n in -60_i64..=60 {
            for d in [-5_i64, -2, -1, 1, 2, 5] {
                let (Ok(f), Ok(c)) = (div_floor(n, d), div_ceil(n, d)) else {
                    continue;
                };
                let inexact = n % d != 0;
                if inexact {
                    assert_eq!(c, f + 1, "{n}/{d}: floor {f} and ceiling {c}");
                } else {
                    assert_eq!(c, f, "{n}/{d} is exact but floor and ceiling differ");
                }
            }
        }
    }

    #[test]
    fn dispatch_matches_the_direct_calls() {
        for n in -20_i64..=20 {
            for d in [-7_i64, -3, -1, 1, 3, 7] {
                assert_eq!(div(n, d, Rounding::Floor), div_floor(n, d));
                assert_eq!(div(n, d, Rounding::Ceiling), div_ceil(n, d));
                assert_eq!(div(n, d, Rounding::TowardZero), div_trunc(n, d));
                assert_eq!(div(n, d, Rounding::HalfUp), div_round_half_up(n, d));
            }
        }
    }

    /// The three modes must agree whenever the division is exact. Disagreement
    /// means one of them has an off-by-one in its correction step.
    #[test]
    fn modes_agree_on_exact_divisions() {
        for d in [-9_i64, -4, -1, 1, 4, 9] {
            for q in -50_i64..=50 {
                let n = q * d;
                assert_eq!(div_floor(n, d), Ok(q), "floor {n}/{d}");
                assert_eq!(div_ceil(n, d), Ok(q), "ceiling {n}/{d}");
                assert_eq!(div_trunc(n, d), Ok(q), "trunc {n}/{d}");
                assert_eq!(div_round_half_up(n, d), Ok(q), "half-up {n}/{d}");
            }
        }
    }

    /// Floor and truncation may differ by at most one, and only when the exact
    /// quotient is both negative and inexact.
    #[test]
    fn floor_and_trunc_differ_only_where_expected() {
        for n in -60_i64..=60 {
            for d in [-5_i64, -2, -1, 1, 2, 5] {
                let (Ok(f), Ok(t)) = (div_floor(n, d), div_trunc(n, d)) else {
                    continue;
                };
                let inexact = n % d != 0;
                let negative = (n < 0) != (d < 0);
                let expected = if inexact && negative { t - 1 } else { t };
                assert_eq!(f, expected, "{n}/{d}");
            }
        }
    }
}
