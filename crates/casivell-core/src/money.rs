//! [`Money`]: an exact monetary amount, stored as an integer number of cents.
//!
//! # Design commitments
//!
//! - **No floating point.** See the crate-level docs for why.
//! - **No panicking operators.** `Add`/`Sub`/`Mul` are deliberately *not*
//!   implemented. Every operation is a named method returning [`Result`], so
//!   overflow is a value the caller must handle rather than an abort. This is
//!   verbose at the call site on purpose: JPL Power-of-10 R7 asks that the return
//!   value of every non-void function be checked, and the easiest way to make
//!   that happen is to leave no unchecked alternative available.
//! - **A bounded domain.** Amounts are constrained to [`Money::MAX_ABS_CENTS`].
//!   Bounding the domain is what lets the tariff evaluator in `casivell-tax`
//!   prove statically that its intermediate products fit in `i64` (R2: all loops
//!   and quantities have a fixed upper bound).

use core::fmt;

use crate::rate::Rate;
use crate::rounding::{Rounding, div};

/// Anything that can go wrong in exact monetary arithmetic.
///
/// Deliberately small and non-generic: an error type that a `#![no_std]` engine
/// can return from a hot loop without allocating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MoneyError {
    /// The result fell outside [`Money::MAX_ABS_CENTS`], or an intermediate
    /// product exceeded `i64`/`i128`.
    Overflow,
    /// A divisor was zero.
    DivisionByZero,
    /// A value was constructed outside the representable domain.
    OutOfDomain {
        /// The offending amount, in cents.
        cents: i64,
    },
    /// No verified statutory parameter set exists for the requested year.
    YearOutOfRange {
        /// The requested calendar year.
        year: u16,
    },
    /// A rate was outside its documented plausible band, which almost always
    /// means a unit mix-up (percent supplied where parts-per-million was meant).
    RateOutOfDomain {
        /// The offending rate, in parts per million.
        ppm: i64,
    },
}

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Overflow => {
                f.write_str("monetary arithmetic overflowed the representable domain")
            }
            Self::DivisionByZero => f.write_str("division by zero"),
            Self::OutOfDomain { cents } => {
                write!(
                    f,
                    "amount {cents} cents is outside the representable domain"
                )
            }
            Self::YearOutOfRange { year } => {
                write!(f, "no verified statutory parameters for year {year}")
            }
            Self::RateOutOfDomain { ppm } => {
                write!(f, "rate {ppm} ppm is outside the plausible domain")
            }
        }
    }
}

impl core::error::Error for MoneyError {}

/// An exact monetary amount in euro cents.
///
/// Negative values are legitimate and represent debts, losses, or refunds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money {
    cents: i64,
}

impl Money {
    /// Cents per euro. Named so the constant never appears as a bare `100`.
    pub const CENTS_PER_EURO: i64 = 100;

    /// The widest amount Casivell will represent: ±10 billion euro.
    ///
    /// Chosen to be absurdly generous for a household — roughly a thousand times
    /// the largest private fortune in Germany — while leaving nine orders of
    /// magnitude of headroom below `i64::MAX`. That headroom is not slack; the
    /// overflow proofs in `casivell-tax::tariff` depend on it.
    pub const MAX_ABS_CENTS: i64 = 1_000_000_000_000;

    /// Zero euro.
    pub const ZERO: Self = Self { cents: 0 };

    /// Constructs an amount from cents.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `|cents|` exceeds [`Self::MAX_ABS_CENTS`].
    pub const fn from_cents(cents: i64) -> Result<Self, MoneyError> {
        if cents > Self::MAX_ABS_CENTS || cents < -Self::MAX_ABS_CENTS {
            return Err(MoneyError::OutOfDomain { cents });
        }
        Ok(Self { cents })
    }

    /// Constructs an amount from whole euro.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if the amount is too large to represent.
    pub const fn from_euro(euro: i64) -> Result<Self, MoneyError> {
        match euro.checked_mul(Self::CENTS_PER_EURO) {
            Some(cents) => Self::from_cents(cents),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Constructs an amount from a euro-and-cent pair, as it would be written.
    ///
    /// `Money::from_euro_cents(1_234, 56)` is `1.234,56 €`. The sign is taken
    /// from `euro`; `cents` is always the magnitude of the fractional part, so
    /// `from_euro_cents(-5, 50)` is `-5,50 €` and not `-4,50 €`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `cents` is not in `0..100`, or if the
    /// amount is too large to represent.
    pub const fn from_euro_cents(euro: i64, cents: u8) -> Result<Self, MoneyError> {
        // `u8` to `i64` is a widening conversion and so cannot lose information.
        // `i64::from` would say that without a cast, but `From` is not yet const,
        // and this constructor is `const` so that the statutory tables in
        // `casivell-lawdata` can be built at compile time.
        let fraction = cents as i64;
        if fraction >= Self::CENTS_PER_EURO {
            return Err(MoneyError::OutOfDomain { cents: fraction });
        }
        let Some(whole) = euro.checked_mul(Self::CENTS_PER_EURO) else {
            return Err(MoneyError::Overflow);
        };
        let signed_fraction = if euro < 0 {
            // `checked_neg` rather than unary minus: `fraction` is provably in
            // `0..100` here, so this cannot fail, but the engine uses no unchecked
            // arithmetic anywhere and an exception would have to be justified
            // rather than merely be correct.
            match fraction.checked_neg() {
                Some(v) => v,
                None => return Err(MoneyError::Overflow),
            }
        } else {
            fraction
        };
        match whole.checked_add(signed_fraction) {
            Some(total) => Self::from_cents(total),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Returns the amount in cents.
    #[must_use]
    pub const fn cents(self) -> i64 {
        self.cents
    }

    /// Returns whether the amount is exactly zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.cents == 0
    }

    /// Returns whether the amount is strictly negative.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.cents < 0
    }

    /// Adds two amounts.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn add(self, other: Self) -> Result<Self, MoneyError> {
        // Both operands are within ±MAX_ABS_CENTS by construction, so the sum
        // cannot overflow i64; only the domain check below can reject it.
        match self.cents.checked_add(other.cents) {
            Some(sum) => Self::from_cents(sum),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Subtracts `other` from `self`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the difference leaves the representable domain.
    pub const fn sub(self, other: Self) -> Result<Self, MoneyError> {
        match self.cents.checked_sub(other.cents) {
            Some(diff) => Self::from_cents(diff),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Negates the amount.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] only in the unreachable case of a value outside
    /// the domain invariant; returned rather than asserted so the function stays
    /// total.
    pub const fn neg(self) -> Result<Self, MoneyError> {
        match self.cents.checked_neg() {
            Some(v) => Self::from_cents(v),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Multiplies by a whole number, such as a count of months.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the product leaves the representable domain.
    pub const fn mul_int(self, factor: i64) -> Result<Self, MoneyError> {
        match self.cents.checked_mul(factor) {
            Some(product) => Self::from_cents(product),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Applies a [`Rate`], rounding as named.
    ///
    /// The intermediate product is computed in `i128`: cents can reach 1e12 and
    /// a rate 1e7 ppm, whose product is 1e19 and would overflow `i64`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the result leaves the representable domain.
    pub const fn mul_rate(self, rate: Rate, mode: Rounding) -> Result<Self, MoneyError> {
        let scaled = (self.cents as i128).saturating_mul(rate.ppm() as i128);
        let per_million = Rate::ONE.ppm() as i128;
        // Narrow before dividing: `scaled` is bounded by 1e12 * 1e7 = 1e19,
        // which exceeds i64 but is far inside i128, and the quotient is bounded
        // by 1e12 * 1e7 / 1e6 = 1e13, comfortably inside i64.
        let quotient = match narrow_i128_div(scaled, per_million, mode) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        Self::from_cents(quotient)
    }

    /// Divides the amount into `parts` equal shares, rounding as named.
    ///
    /// Used for the employer/employee halving of social-insurance contributions.
    /// The shares will not always sum back to the original; where statute cares
    /// about the residual cent, the caller must compute one share and subtract.
    ///
    /// # Errors
    ///
    /// [`MoneyError::DivisionByZero`] if `parts == 0`, [`MoneyError::Overflow`]
    /// on a non-representable result.
    pub const fn div_int(self, parts: i64, mode: Rounding) -> Result<Self, MoneyError> {
        match div(self.cents, parts, mode) {
            Ok(v) => Self::from_cents(v),
            Err(e) => Err(e),
        }
    }

    /// Truncates toward negative infinity to a whole euro amount.
    ///
    /// This is *abrunden auf einen vollen Euro-Betrag*, required by
    /// [§ 32a Abs. 1 EStG] both for the taxable income going in and the tax
    /// coming out.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] on a non-representable result.
    ///
    /// [§ 32a Abs. 1 EStG]: https://www.gesetze-im-internet.de/estg/__32a.html
    pub const fn floor_to_euro(self) -> Result<Self, MoneyError> {
        let euro = match div(self.cents, Self::CENTS_PER_EURO, Rounding::Floor) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        Self::from_euro(euro)
    }

    /// Returns the amount as whole euro, truncated toward negative infinity.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] on a non-representable result.
    pub const fn whole_euro_floor(self) -> Result<i64, MoneyError> {
        div(self.cents, Self::CENTS_PER_EURO, Rounding::Floor)
    }

    /// Returns the larger of two amounts.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.cents >= other.cents {
            self
        } else {
            other
        }
    }

    /// Returns the smaller of two amounts.
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        if self.cents <= other.cents {
            self
        } else {
            other
        }
    }

    /// Clamps into `[lo, hi]`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `lo > hi`, which is a caller bug rather
    /// than a runtime condition and so is surfaced rather than silently fixed.
    pub const fn clamp(self, lo: Self, hi: Self) -> Result<Self, MoneyError> {
        if lo.cents > hi.cents {
            return Err(MoneyError::OutOfDomain { cents: lo.cents });
        }
        Ok(self.max(lo).min(hi))
    }

    /// Replaces a negative amount with zero.
    ///
    /// Statutes routinely say a computed figure is "höchstens" some amount or
    /// cannot fall below zero; this names that operation instead of scattering
    /// `max(ZERO)` around.
    #[must_use]
    pub const fn floor_at_zero(self) -> Self {
        self.max(Self::ZERO)
    }
}

/// Divides an `i128` by an `i128` with the named rounding, then narrows to `i64`.
///
/// Exists because [`crate::rounding`] is deliberately `i64`-only — that is the
/// width the whole engine speaks — while [`Money::mul_rate`] needs one wider
/// intermediate step. Keeping the widening local to this one function means the
/// `i128` never escapes into the rest of the engine.
const fn narrow_i128_div(n: i128, d: i128, mode: Rounding) -> Result<i64, MoneyError> {
    if d == 0 {
        return Err(MoneyError::DivisionByZero);
    }
    let (Some(q), Some(r)) = (n.checked_div(d), n.checked_rem(d)) else {
        return Err(MoneyError::Overflow);
    };
    let adjusted = match mode {
        Rounding::TowardZero => q,
        Rounding::Floor => {
            if r != 0 && ((r < 0) != (d < 0)) {
                match q.checked_sub(1) {
                    Some(v) => v,
                    None => return Err(MoneyError::Overflow),
                }
            } else {
                q
            }
        }
        Rounding::HalfUp => {
            let Some(twice) = r.checked_mul(2) else {
                return Err(MoneyError::Overflow);
            };
            if twice.unsigned_abs() < d.unsigned_abs() {
                q
            } else {
                let step = if (n < 0) == (d < 0) { 1 } else { -1 };
                match q.checked_add(step) {
                    Some(v) => v,
                    None => return Err(MoneyError::Overflow),
                }
            }
        }
    };
    // Widening `i64::MAX`/`i64::MIN` to `i128` is lossless, so this comparison is
    // exact and establishes that the narrowing below cannot lose information.
    if adjusted > i64::MAX as i128 || adjusted < i64::MIN as i128 {
        return Err(MoneyError::Overflow);
    }
    // `i64::try_from` would express this without a cast but is not `const`-callable,
    // and this function is `const` so that `Money::mul_rate` can be. The bounds
    // check immediately above is the proof the lint is asking for. `expect` rather
    // than `allow`: if a future edit makes the cast unnecessary, the unfulfilled
    // expectation fails the build and this comment gets deleted with it.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the preceding bounds check proves `adjusted` is within i64"
    )]
    Ok(adjusted as i64)
}

#[cfg(test)]
mod tests {
    use super::{Money, MoneyError};
    use crate::rate::Rate;
    use crate::rounding::Rounding;

    #[test]
    fn euro_and_cent_constructors_agree() {
        assert_eq!(Money::from_euro(12).unwrap().cents(), 1_200);
        assert_eq!(Money::from_euro_cents(1_234, 56).unwrap().cents(), 123_456);
        assert_eq!(Money::from_cents(123_456).unwrap().cents(), 123_456);
    }

    /// A negative euro-and-cent pair reads as a magnitude with a sign, the way a
    /// human writes it. Getting this backwards would put every debt one euro out.
    #[test]
    fn negative_amounts_take_the_sign_from_the_euro_field() {
        assert_eq!(Money::from_euro_cents(-5, 50).unwrap().cents(), -550);
        assert_eq!(Money::from_euro_cents(-0, 50).unwrap().cents(), 50);
    }

    #[test]
    fn rejects_a_fractional_part_of_a_hundred_or_more() {
        assert!(matches!(
            Money::from_euro_cents(1, 100),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    #[test]
    fn rejects_amounts_outside_the_domain() {
        let too_big = Money::MAX_ABS_CENTS.checked_add(1).unwrap();
        assert!(matches!(
            Money::from_cents(too_big),
            Err(MoneyError::OutOfDomain { .. })
        ));
        assert!(Money::from_cents(Money::MAX_ABS_CENTS).is_ok());
        assert!(Money::from_cents(-Money::MAX_ABS_CENTS).is_ok());
    }

    #[test]
    fn addition_and_subtraction_are_exact() {
        let a = Money::from_euro_cents(0, 10).unwrap();
        let b = Money::from_euro_cents(0, 20).unwrap();
        assert_eq!(a.add(b).unwrap().cents(), 30);
        assert_eq!(a.sub(b).unwrap().cents(), -10);
    }

    /// The canonical floating-point failure: 0.1 + 0.2 != 0.3. Summing ten cents
    /// three times must land exactly on thirty, and repeating it a thousand times
    /// must not drift by a single cent.
    #[test]
    fn repeated_addition_never_drifts() {
        let ten_cents = Money::from_cents(10).unwrap();
        let mut total = Money::ZERO;
        for _ in 0..1_000 {
            total = total.add(ten_cents).unwrap();
        }
        assert_eq!(total.cents(), 10_000);
    }

    /// Leaving the domain and overflowing the machine word are different faults
    /// and are reported differently: the first means the *amount* is implausible,
    /// the second that an intermediate could not be represented at all. Conflating
    /// them would make the domain bound untestable.
    #[test]
    fn leaving_the_domain_is_reported_separately_from_machine_overflow() {
        let max = Money::from_cents(Money::MAX_ABS_CENTS).unwrap();

        // 2 · MAX_ABS_CENTS fits in an i64 but is outside the declared domain.
        assert_eq!(
            max.add(max),
            Err(MoneyError::OutOfDomain {
                cents: Money::MAX_ABS_CENTS * 2
            })
        );
        assert!(matches!(
            max.mul_int(3),
            Err(MoneyError::OutOfDomain { .. })
        ));

        // A factor large enough to exceed i64 itself is machine overflow.
        assert_eq!(max.mul_int(i64::MAX), Err(MoneyError::Overflow));
    }

    /// Neither fault may panic, at any magnitude. This is the property that lets
    /// the engine run with `panic = "abort"` in release and still be trusted not
    /// to abort on user input.
    #[test]
    fn no_arithmetic_panics_at_any_magnitude() {
        let extremes = [
            i64::MIN,
            i64::MIN + 1,
            -Money::MAX_ABS_CENTS,
            -1,
            0,
            1,
            Money::MAX_ABS_CENTS,
            i64::MAX - 1,
            i64::MAX,
        ];
        for cents in extremes {
            // Construction either succeeds or reports; it never panics.
            let Ok(amount) = Money::from_cents(cents) else {
                continue;
            };
            for other in extremes {
                let Ok(rhs) = Money::from_cents(other) else {
                    continue;
                };
                let _ = amount.add(rhs);
                let _ = amount.sub(rhs);
            }
            for factor in extremes {
                let _ = amount.mul_int(factor);
                let _ = amount.div_int(factor, Rounding::Floor);
            }
            let _ = amount.neg();
            let _ = amount.floor_to_euro();
            let _ = amount.whole_euro_floor();
            let _ = amount.mul_rate(Rate::ONE, Rounding::HalfUp);
        }
    }

    #[test]
    fn floor_to_euro_truncates_downward_on_both_signs() {
        assert_eq!(
            Money::from_euro_cents(12, 99).unwrap().floor_to_euro(),
            Money::from_euro(12)
        );
        // -12,99 € floors to -13 €, away from zero. Truncation toward zero would
        // give -12 € and quietly favour the taxpayer by a euro.
        assert_eq!(
            Money::from_euro_cents(-12, 99).unwrap().floor_to_euro(),
            Money::from_euro(-13)
        );
    }

    #[test]
    fn rate_application_rounds_as_named() {
        let base = Money::from_euro(1_000).unwrap();
        // 5.5 % of 1 000,00 € is exactly 55,00 €.
        let soli = Rate::from_percent_millis(5_500).unwrap();
        assert_eq!(
            base.mul_rate(soli, Rounding::HalfUp).unwrap().cents(),
            5_500
        );

        // 14.6 % of 3,33 € is 0.48618 €; 48 cents flooring, 49 rounding.
        let gkv = Rate::from_percent_millis(14_600).unwrap();
        let odd = Money::from_euro_cents(3, 33).unwrap();
        assert_eq!(odd.mul_rate(gkv, Rounding::Floor).unwrap().cents(), 48);
        assert_eq!(odd.mul_rate(gkv, Rounding::HalfUp).unwrap().cents(), 49);
    }

    /// `mul_rate` must survive the widest amount times the largest rate, which is
    /// the case that would overflow an `i64` intermediate.
    #[test]
    fn rate_application_survives_the_domain_corners() {
        let max = Money::from_cents(Money::MAX_ABS_CENTS).unwrap();
        let full = Rate::ONE;
        assert_eq!(max.mul_rate(full, Rounding::Floor).unwrap(), max);

        let half = Rate::from_percent_millis(50_000).unwrap();
        assert_eq!(
            max.mul_rate(half, Rounding::Floor).unwrap().cents(),
            Money::MAX_ABS_CENTS / 2
        );
    }

    #[test]
    fn halving_a_contribution_leaves_the_odd_cent_to_the_caller() {
        let odd = Money::from_cents(101).unwrap();
        let employee = odd.div_int(2, Rounding::Floor).unwrap();
        assert_eq!(employee.cents(), 50);
        // The residual cent is recovered by subtraction, never by a second
        // rounding, so the two shares always reconstruct the whole.
        let employer = odd.sub(employee).unwrap();
        assert_eq!(employer.cents(), 51);
        assert_eq!(employee.add(employer).unwrap(), odd);
    }

    #[test]
    fn clamp_rejects_an_inverted_interval() {
        let lo = Money::from_euro(10).unwrap();
        let hi = Money::from_euro(1).unwrap();
        assert!(matches!(
            Money::ZERO.clamp(lo, hi),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    #[test]
    fn floor_at_zero_clears_only_negatives() {
        assert_eq!(Money::from_euro(-5).unwrap().floor_at_zero(), Money::ZERO);
        assert_eq!(
            Money::from_euro(5).unwrap().floor_at_zero().cents(),
            Money::from_euro(5).unwrap().cents()
        );
    }

    #[test]
    fn ordering_follows_the_numeric_value() {
        let mut amounts = [
            Money::from_euro(5).unwrap(),
            Money::from_euro(-5).unwrap(),
            Money::ZERO,
        ];
        amounts.sort_unstable();
        assert_eq!(amounts[0].cents(), -500);
        assert_eq!(amounts[1].cents(), 0);
        assert_eq!(amounts[2].cents(), 500);
    }
}
