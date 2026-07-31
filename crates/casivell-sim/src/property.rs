//! A home the household owns, as the kernel carries it.
//!
//! Kept out of [`crate::timeline`] because a purchase is the one event with genuine *state* —
//! a balance that falls, a value that moves — rather than a per-month input the schedule can
//! resolve on its own.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_property::{Amortisation, PurchaseCosts};

/// A property and its mortgage, carried from month to month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnedProperty {
    /// What it cost to buy, itemised.
    pub costs: PurchaseCosts,
    /// The loan's schedule, fixed at completion.
    pub loan: Amortisation,

    /// The property's value now, which starts at the price and moves with the assumption.
    pub value: Money,
    /// What is still owed.
    pub balance: Money,
    /// Interest paid so far.
    pub interest_paid: Money,
    /// Principal repaid so far.
    pub principal_repaid: Money,
}

impl OwnedProperty {
    /// A property just bought.
    #[must_use]
    pub const fn just_bought(costs: PurchaseCosts, loan: Amortisation) -> Self {
        Self {
            costs,
            loan,
            value: costs.price,
            balance: costs.loan_required,
            interest_paid: Money::ZERO,
            principal_repaid: Money::ZERO,
        }
    }

    /// The household's equity: what the property would leave after clearing the mortgage.
    ///
    /// Before any costs of selling, which are not modelled. Negative where the property is
    /// worth less than the debt — which is a real state and reported rather than floored,
    /// because a household that is under water needs to see it.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub const fn equity(&self) -> Result<Money, MoneyError> {
        self.value.sub(self.balance)
    }

    /// Whether the mortgage is still running.
    #[must_use]
    pub const fn owes(&self) -> bool {
        !self.balance.is_zero()
    }

    /// Advances one month: charges interest, repays principal, revalues the property.
    ///
    /// Returns the payment actually made, which is the full annuity until the last month and
    /// whatever remains in it — and zero once the loan has cleared, at which point the
    /// household's housing cost falls to its running costs alone.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub fn advance(&mut self, monthly_growth: Rate) -> Result<Money, MoneyError> {
        // Revalue first, so a month's growth applies to the value held at its start — the
        // same convention `advance_wealth` uses for investment return.
        let appreciation = self.value.mul_rate(monthly_growth, Rounding::HalfUp)?;
        self.value = self.value.add(appreciation)?;

        if !self.owes() {
            return Ok(Money::ZERO);
        }

        let interest = monthly_interest(self.balance, self.costs_interest_rate())?;
        let repayment = self
            .loan
            .monthly_payment
            .sub(interest)?
            .floor_at_zero()
            .min(self.balance);
        self.balance = self.balance.sub(repayment)?;
        self.interest_paid = self.interest_paid.add(interest)?;
        self.principal_repaid = self.principal_repaid.add(repayment)?;

        interest.add(repayment)
    }

    /// The loan's interest rate, recovered from the schedule.
    ///
    /// Stored on the amortisation rather than duplicated here, so the rate the schedule was
    /// built from and the rate charged each month cannot disagree.
    const fn costs_interest_rate(&self) -> Rate {
        self.loan.interest_rate
    }
}

/// One month's interest, a twelfth of the annual rate.
fn monthly_interest(balance: Money, annual_rate: Rate) -> Result<Money, MoneyError> {
    balance
        .mul_rate(annual_rate, Rounding::HalfUp)?
        .div_int(12, Rounding::HalfUp)
}
