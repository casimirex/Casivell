//! The annuity mortgage, amortised month by month.
//!
//! Exact arithmetic on stated contract terms — no assumptions beyond what the borrower was
//! offered. The one thing that has to be chosen is the rounding of the monthly interest, and
//! that is named where it happens.

use casivell_core::{Money, MoneyError, Rate, Rounding};

/// Months in a year.
const MONTHS_PER_YEAR: u32 = 12;

/// The longest schedule the type will run: sixty years.
///
/// A bound so the amortisation loop has a provable upper limit (JPL R2). No German mortgage
/// runs near it; a loan that has not cleared by then is one whose payment does not cover its
/// interest, which [`amortise`] refuses outright.
const MAX_MONTHS: u32 = 60 * MONTHS_PER_YEAR;

/// The terms of a loan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MortgageTerms {
    /// The amount borrowed.
    pub principal: Money,
    /// The Sollzins, annually.
    pub interest_rate: Rate,
    /// The anfängliche Tilgung, annually — the share of the *original* principal repaid in
    /// the first year.
    ///
    /// German mortgages are quoted this way rather than by term. Two percent on a 3,5 % loan
    /// clears in about twenty-eight years, not fifty, because the repayment portion grows as
    /// the interest portion shrinks.
    pub initial_repayment_rate: Rate,
    /// The Zinsbindung in whole years, after which the rate is renegotiated.
    ///
    /// Not the term of the loan. A ten-year fix on a twenty-eight-year amortisation leaves a
    /// balance to refinance at whatever rates then are.
    pub fixed_years: u32,
}

/// One month of a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MortgageMonth {
    /// Months elapsed, counting from zero.
    pub month_index: u32,
    /// The fixed monthly payment.
    pub payment: Money,
    /// The part of it that was interest.
    pub interest: Money,
    /// The part that reduced the balance.
    pub repayment: Money,
    /// What is left owing at the end of the month.
    pub balance: Money,
}

/// The result of amortising a loan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amortisation {
    /// The fixed monthly payment.
    pub monthly_payment: Money,
    /// Months until the balance reaches zero.
    pub months_to_clear: u32,
    /// Interest paid over the whole life of the loan.
    ///
    /// The number that makes a household reconsider the Tilgung: at 3,5 % and 2 % a 368 280 €
    /// loan pays more than 200 000 € of interest before it clears.
    pub total_interest: Money,

    /// The balance still owing when the Zinsbindung ends.
    pub balance_at_fix_end: Money,
    /// Interest paid during the fixed period.
    pub interest_during_fix: Money,
    /// The rate the schedule was built from.
    ///
    /// Carried on the result so a caller re-running the amortisation month by month — the
    /// simulation kernel does — charges the same rate the schedule assumed, rather than being
    /// handed the terms separately and able to disagree with them.
    pub interest_rate: Rate,
}

/// The monthly payment for a set of terms.
///
/// The German convention: `(Zins + Tilgung) × principal ÷ 12`.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn monthly_payment(terms: &MortgageTerms) -> Result<Money, MoneyError> {
    let combined = terms.interest_rate.add(terms.initial_repayment_rate)?;
    let annual = terms.principal.mul_rate(combined, Rounding::HalfUp)?;
    annual.div_int(i64::from(MONTHS_PER_YEAR), Rounding::HalfUp)
}

/// Amortises a loan to the cent.
///
/// # Errors
///
/// [`MoneyError::OutOfDomain`] if the payment does not cover the first month's interest, in
/// which case the balance would grow forever and there is no schedule to compute. That is a
/// real condition rather than a guard against overflow: a Tilgung of zero, which some
/// interest-only products use, produces exactly it.
pub fn amortise(terms: &MortgageTerms) -> Result<Amortisation, MoneyError> {
    let payment = monthly_payment(terms)?;
    let mut balance = terms.principal.floor_at_zero();

    if balance.is_zero() {
        return Ok(Amortisation {
            monthly_payment: payment,
            months_to_clear: 0,
            total_interest: Money::ZERO,
            balance_at_fix_end: Money::ZERO,
            interest_during_fix: Money::ZERO,
            interest_rate: terms.interest_rate,
        });
    }

    // The loan never clears unless the first payment beats the first month's interest, and
    // every later month is easier than the first. Refused rather than looped.
    let first_interest = monthly_interest(balance, terms.interest_rate)?;
    if payment <= first_interest {
        return Err(MoneyError::OutOfDomain {
            cents: payment.cents(),
        });
    }

    let fix_ends_at = terms.fixed_years.saturating_mul(MONTHS_PER_YEAR);
    let mut total_interest = Money::ZERO;
    let mut interest_during_fix = Money::ZERO;
    let mut balance_at_fix_end = Money::ZERO;
    let mut months_to_clear = 0;

    for month_index in 0..MAX_MONTHS {
        if balance.is_zero() {
            break;
        }
        let interest = monthly_interest(balance, terms.interest_rate)?;
        // The final payment is only what is left, not the full annuity.
        let repayment = payment.sub(interest)?.min(balance);
        balance = balance.sub(repayment)?;

        total_interest = total_interest.add(interest)?;
        if month_index < fix_ends_at {
            interest_during_fix = interest_during_fix.add(interest)?;
        }
        if month_index.saturating_add(1) == fix_ends_at {
            balance_at_fix_end = balance;
        }
        months_to_clear = month_index.saturating_add(1);
    }

    Ok(Amortisation {
        monthly_payment: payment,
        months_to_clear,
        total_interest,
        balance_at_fix_end,
        interest_during_fix,
        interest_rate: terms.interest_rate,
    })
}

/// One month's interest on a balance.
///
/// A twelfth of the annual rate, which is the German convention and not the same as the
/// twelfth root of the annual factor. Rounded half up to the cent, as a lender's statement is.
fn monthly_interest(balance: Money, annual_rate: Rate) -> Result<Money, MoneyError> {
    let annual = balance.mul_rate(annual_rate, Rounding::HalfUp)?;
    annual.div_int(i64::from(MONTHS_PER_YEAR), Rounding::HalfUp)
}

impl Amortisation {
    /// The balance still owing after `months`.
    ///
    /// Zero once the loan has cleared.
    #[must_use]
    pub const fn remaining_at(&self, months: u32) -> Money {
        if months >= self.months_to_clear {
            return Money::ZERO;
        }
        self.balance_at_fix_end
    }

    /// Years and months until the loan clears, for a report.
    #[must_use]
    pub const fn term(&self) -> (u32, u32) {
        (
            self.months_to_clear / MONTHS_PER_YEAR,
            self.months_to_clear % MONTHS_PER_YEAR,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{MortgageTerms, amortise, monthly_payment};
    use casivell_core::{Money, MoneyError, Rate};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn pct(value: i64) -> Rate {
        Rate::from_percent_millis(value).unwrap()
    }

    fn terms(principal: i64, interest: i64, repayment: i64, fixed: u32) -> MortgageTerms {
        MortgageTerms {
            principal: euro(principal),
            interest_rate: pct(interest),
            initial_repayment_rate: pct(repayment),
            fixed_years: fixed,
        }
    }

    /// The German convention, checked by hand: 3,5 % + 2 % on 400 000 € is 22 000 € a year,
    /// or 1 833,33 € a month.
    #[test]
    fn the_payment_follows_the_german_convention() {
        let payment = monthly_payment(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        assert_eq!(
            payment,
            euro(1_833).add(Money::from_cents(33).unwrap()).unwrap()
        );
    }

    /// The property that surprises people: 2 % Tilgung does not mean fifty years.
    ///
    /// The payment is fixed, so as the interest portion falls the repayment portion grows,
    /// and the loan clears in **29 years** rather than the fifty a naive `100 / 2` suggests.
    #[test]
    fn two_percent_tilgung_clears_in_twenty_nine_years_not_fifty() {
        let result = amortise(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        assert_eq!(result.term(), (29, 0));
        assert!(
            result.months_to_clear < 50 * 12,
            "the naive 100/2 reading would say fifty years"
        );
    }

    /// Raising the Tilgung shortens the loan and cuts the interest sharply — the largest
    /// lever a borrower has after the rate itself.
    ///
    /// On 400 000 € at 3,5 %, one percentage point more Tilgung takes the term from **29
    /// years to 22** and the interest from **236 792 € to 175 207 €** — a saving of 61 585 €,
    /// or about 26 %, for 333 € more a month. Figures pinned rather than bounded, because the
    /// point of the test is the size of the effect and a loose bound would not show it.
    #[test]
    fn a_higher_tilgung_costs_far_less_interest() {
        let slow = amortise(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        let fast = amortise(&terms(400_000, 3_500, 3_000, 10)).unwrap();

        assert_eq!(slow.term(), (29, 0));
        assert_eq!(fast.term(), (22, 2));
        assert_eq!(slow.total_interest, Money::from_cents(23_679_190).unwrap());
        assert_eq!(fast.total_interest, Money::from_cents(17_520_736).unwrap());

        // A quarter of the interest, bought with a third more payment each month.
        let saved = slow.total_interest.sub(fast.total_interest).unwrap();
        assert!(saved.cents() * 4 > slow.total_interest.cents());
        assert!(fast.monthly_payment > slow.monthly_payment);
    }

    /// The residual at the end of the Zinsbindung: the number a household most needs and
    /// least often sees. Ten years into a twenty-eight-year loan, most of it is still owed.
    #[test]
    fn the_fixed_period_leaves_most_of_the_loan_outstanding() {
        let result = amortise(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        let remaining = result.balance_at_fix_end;

        assert!(
            remaining > euro(300_000),
            "still {remaining:?} after ten years"
        );
        assert!(remaining < euro(320_000));

        // Which is to say: ten years of payments retired under a quarter of the debt, while
        // more than half of everything paid in that decade went to interest.
        let retired = euro(400_000).sub(remaining).unwrap();
        assert!(retired.cents() * 4 < euro(400_000).cents());

        let paid_in_ten_years = result.monthly_payment.mul_int(120).unwrap();
        assert!(
            result.interest_during_fix.cents() * 2 > paid_in_ten_years.cents(),
            "most of a decade's payments should still be interest"
        );
    }

    /// Over the life of the loan the interest is comparable to the principal itself.
    #[test]
    fn total_interest_is_of_the_same_order_as_the_loan() {
        let result = amortise(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        assert!(result.total_interest > euro(200_000));
        assert!(result.total_interest < euro(400_000));
    }

    /// Every schedule must reconcile: payments made, less interest, equals the principal.
    #[test]
    fn the_schedule_reconciles_to_the_principal() {
        for (principal, interest, repayment) in [
            (400_000_i64, 3_500_i64, 2_000_i64),
            (250_000, 4_500, 3_000),
            (120_000, 2_000, 5_000),
        ] {
            let result = amortise(&terms(principal, interest, repayment, 10)).unwrap();
            // All payments but the last are the full annuity; the last is whatever remained.
            // So total paid lies between (n-1) and n annuities, and paid less interest is the
            // principal to within that last month's rounding.
            let full = result
                .monthly_payment
                .mul_int(i64::from(result.months_to_clear))
                .unwrap();
            let implied_principal = full.sub(result.total_interest).unwrap();
            let difference = implied_principal.sub(euro(principal)).unwrap();
            assert!(
                difference.cents().abs() <= result.monthly_payment.cents(),
                "at {principal}/{interest}/{repayment} the schedule was out by {difference:?}"
            );
        }
    }

    /// A payment that cannot cover the first month's interest has no schedule, and saying so
    /// is better than looping to the bound and reporting a loan that never clears.
    #[test]
    fn a_loan_that_cannot_amortise_is_refused() {
        // Zero Tilgung: the payment is exactly the interest, so the balance never moves.
        assert!(matches!(
            amortise(&terms(400_000, 3_500, 0, 10)),
            Err(MoneyError::OutOfDomain { .. })
        ));
    }

    /// No loan means no schedule rather than an error.
    #[test]
    fn a_zero_loan_amortises_trivially() {
        let result = amortise(&terms(0, 3_500, 2_000, 10)).unwrap();
        assert_eq!(result.months_to_clear, 0);
        assert_eq!(result.total_interest, Money::ZERO);
    }

    /// A fix longer than the loan leaves nothing outstanding.
    #[test]
    fn a_fix_outlasting_the_loan_leaves_no_residual() {
        let result = amortise(&terms(120_000, 2_000, 10_000, 40)).unwrap();
        assert_eq!(result.balance_at_fix_end, Money::ZERO);
        assert!(result.months_to_clear < 40 * 12);
    }

    /// Interest during the fix must be a part of the whole, never more.
    #[test]
    fn interest_during_the_fix_is_part_of_the_total() {
        let result = amortise(&terms(400_000, 3_500, 2_000, 10)).unwrap();
        assert!(result.interest_during_fix > Money::ZERO);
        assert!(result.interest_during_fix < result.total_interest);
    }
}
