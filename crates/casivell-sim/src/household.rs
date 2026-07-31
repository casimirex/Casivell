//! What is being simulated, and under what assumptions.

use casivell_core::{Money, MoneyError, Rate, TaxYear};
use casivell_payroll::Employment;
use casivell_projection::Assumptions;

use crate::timeline::{Basis, Horizon};

/// A household's starting position and how it changes.
///
/// The baseline is one steady employment growing at a constant rate; [`Household::schedule`]
/// carries the events that depart from it.
#[derive(Debug, Clone, Copy)]
pub struct Household {
    /// The year the projection starts in.
    pub start_year: TaxYear,
    /// The month it starts in, where January is 1.
    ///
    /// Not cosmetic: the Rentenwert changes on 1 July, so a projection that always
    /// started in January would systematically pick one half of the year.
    pub start_month: u8,

    /// The employment's tax and insurance circumstances.
    pub employment: Employment,
    /// Monthly gross pay at the start.
    pub monthly_gross: Money,
    /// Annual nominal pay growth.
    ///
    /// Separate from the wage-growth assumption used to project *statutory* figures. An
    /// individual's pay and the national average need not move together, and conflating
    /// them would hide the most interesting case: someone whose pay lags the ceilings.
    pub annual_pay_growth: Rate,

    /// Monthly expenses at the start.
    pub monthly_expenses: Money,
    /// Annual growth in expenses.
    ///
    /// Usually the price-inflation assumption, but held separately so a household can
    /// model spending that grows faster or slower than prices.
    pub annual_expense_growth: Rate,

    /// Wealth at the start of the projection.
    pub initial_wealth: Money,
    /// Entgeltpunkte already accrued before the projection begins.
    ///
    /// Someone starting a projection at 40 has a pension record already; treating them
    /// as starting from zero would understate their entitlement by half a career.
    pub initial_pension_points: casivell_social::EntgeltPoints,

    /// Life events that depart from the baseline.
    ///
    /// Empty by default, so a household with nothing scheduled behaves exactly as it did
    /// before events existed — which is what makes the addition safe.
    pub schedule: crate::events::Schedule,
}

impl Household {
    /// Describes a household with no accrued wealth or pension record.
    ///
    /// # Errors
    ///
    /// [`MoneyError::OutOfDomain`] if `start_month` is not in `1..=12`.
    pub fn starting_fresh(
        start_year: TaxYear,
        start_month: u8,
        employment: Employment,
        monthly_gross: Money,
        monthly_expenses: Money,
    ) -> Result<Self, MoneyError> {
        if !(1..=12).contains(&start_month) {
            return Err(MoneyError::OutOfDomain {
                cents: i64::from(start_month),
            });
        }
        Ok(Self {
            start_year,
            start_month,
            employment,
            monthly_gross,
            annual_pay_growth: Rate::ZERO,
            monthly_expenses,
            annual_expense_growth: Rate::ZERO,
            initial_wealth: Money::ZERO,
            initial_pension_points: casivell_social::EntgeltPoints::ZERO,
            schedule: crate::events::Schedule::new(),
        })
    }
}

/// How the projection is run.
#[derive(Debug, Clone, Copy)]
pub struct SimulationConfig {
    /// How long to project for.
    pub horizon: Horizon,
    /// The growth assumptions used to project statutory parameters past 2026.
    pub assumptions: Assumptions,
    /// Annual nominal return on accumulated wealth.
    ///
    /// A single deterministic rate. [`crate::monte_carlo()`] replaces it with a draw from a
    /// caller-supplied set of returns.
    pub investment_return: Rate,
    /// Whether output is nominal or deflated to the starting year.
    pub basis: Basis,
    /// Annual nominal growth in the value of owned property.
    ///
    /// Separate from [`Self::investment_return`] because a house is not a portfolio: it is one
    /// undiversified asset in one town, and its return is the assumption a buy-versus-rent
    /// answer is most sensitive to. Held apart so a household can see what changing it does.
    pub property_growth: Rate,
}

impl SimulationConfig {
    /// A configuration with default statutory assumptions and no investment return.
    ///
    /// Zero return is not a neutral default in substance — it says savings lose ground to
    /// inflation every year — which is why it must be chosen rather than inherited.
    #[must_use]
    pub fn conservative(horizon: Horizon, basis: Basis) -> Self {
        Self {
            horizon,
            assumptions: Assumptions::default(),
            investment_return: Rate::ZERO,
            basis,
            property_growth: Rate::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Household;
    use casivell_core::{Money, MoneyError, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_payroll::{Employment, HealthCover};
    use casivell_social::{EntgeltPoints, Insured};

    fn employment() -> Employment {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(
            insured,
            TaxClass::Class1,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_household_starts_from_nothing() {
        let h = Household::starting_fresh(
            TaxYear::new(2026).unwrap(),
            1,
            employment(),
            Money::from_euro(4_000).unwrap(),
            Money::from_euro(2_000).unwrap(),
        )
        .expect("valid");
        assert_eq!(h.initial_wealth, Money::ZERO);
        assert_eq!(h.initial_pension_points, EntgeltPoints::ZERO);
        assert!(h.annual_pay_growth.is_zero());
    }

    /// The start month must be a real month. A projection silently starting in month 13
    /// would pick the wrong Rentenwert for every year.
    #[test]
    fn an_impossible_start_month_is_refused() {
        for month in [0_u8, 13, 200] {
            assert!(matches!(
                Household::starting_fresh(
                    TaxYear::new(2026).unwrap(),
                    month,
                    employment(),
                    Money::from_euro(4_000).unwrap(),
                    Money::ZERO,
                ),
                Err(MoneyError::OutOfDomain { .. })
            ));
        }
        for month in 1_u8..=12 {
            assert!(
                Household::starting_fresh(
                    TaxYear::new(2026).unwrap(),
                    month,
                    employment(),
                    Money::from_euro(4_000).unwrap(),
                    Money::ZERO,
                )
                .is_ok(),
                "month {month} should be accepted"
            );
        }
    }
}
