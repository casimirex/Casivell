//! Kaufnebenkosten: what a purchase costs beyond the price.

use casivell_core::{Money, MoneyError, Rate, Rounding};
use casivell_lawdata::{Bundesland, PropertyCostParameters};

/// The costs of acquiring a property, itemised by how certain each one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseCosts {
    /// The agreed price.
    pub price: Money,

    /// Grunderwerbsteuer. Statutory and exact.
    pub transfer_tax: Money,
    /// The rate that produced it, for a report that wants to name the state's choice.
    pub transfer_tax_rate: Rate,

    /// Notary and land registry. An approximation of the `GNotKG` schedule, not a statutory
    /// figure — see [`PropertyCostParameters::notary_and_registry_rate`].
    pub notary_and_registry: Money,
    /// Maklerprovision, as supplied. Contractual, never statutory.
    pub agent_commission: Money,

    /// Everything above the price.
    pub incidental_total: Money,
    /// Price plus incidentals: what the purchase actually costs.
    pub total: Money,

    /// The buyer's own money.
    pub deposit: Money,
    /// What has to be borrowed.
    ///
    /// Note what this implies: the incidental costs are almost never lent against, because
    /// they buy nothing a bank can take security over. A household with 80 000 € and a
    /// 400 000 € target is not putting down 20 % — after 40 000 € of costs it is putting down
    /// 10 % and borrowing the rest.
    pub loan_required: Money,
}

impl PurchaseCosts {
    /// The incidental costs as a share of the price, which is how they are usually quoted.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub fn incidental_rate(&self) -> Result<Rate, MoneyError> {
        if self.price.is_zero() {
            return Ok(Rate::ZERO);
        }
        let scaled =
            i128::from(self.incidental_total.cents()).saturating_mul(i128::from(Rate::ONE.ppm()));
        let ppm = scaled
            .checked_div(i128::from(self.price.cents()))
            .ok_or(MoneyError::DivisionByZero)?;
        Rate::from_ppm(i64::try_from(ppm).map_err(|_| MoneyError::Overflow)?)
    }

    /// The deposit as a share of the *price*, which is what a lender's loan-to-value uses.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub fn deposit_against_price(&self) -> Result<Rate, MoneyError> {
        if self.price.is_zero() {
            return Ok(Rate::ZERO);
        }
        let scaled = i128::from(self.deposit.cents()).saturating_mul(i128::from(Rate::ONE.ppm()));
        let ppm = scaled
            .checked_div(i128::from(self.price.cents()))
            .ok_or(MoneyError::DivisionByZero)?;
        Rate::from_ppm(i64::try_from(ppm).map_err(|_| MoneyError::Overflow)?)
    }
}

/// Computes the cost of a purchase.
///
/// `agent_commission_rate` is the buyer's share of any Maklerprovision. Zero where there is no
/// agent, which is common for a private sale. § 656c BGB caps a private buyer's share at the
/// seller's, but the rate is negotiated and so is an input rather than a parameter.
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn purchase_costs(
    price: Money,
    land: Bundesland,
    deposit: Money,
    agent_commission_rate: Rate,
    costs: &PropertyCostParameters,
) -> Result<PurchaseCosts, MoneyError> {
    let price = price.floor_at_zero();
    let transfer_tax_rate = costs.transfer_tax_rate(land);

    let transfer_tax = price.mul_rate(transfer_tax_rate, Rounding::HalfUp)?;
    let notary_and_registry = price.mul_rate(costs.notary_and_registry_rate, Rounding::HalfUp)?;
    let agent_commission = price.mul_rate(agent_commission_rate, Rounding::HalfUp)?;

    let incidental_total = transfer_tax
        .add(notary_and_registry)?
        .add(agent_commission)?;
    let total = price.add(incidental_total)?;

    let deposit = deposit.floor_at_zero().min(total);
    Ok(PurchaseCosts {
        price,
        transfer_tax,
        transfer_tax_rate,
        notary_and_registry,
        agent_commission,
        incidental_total,
        total,
        deposit,
        loan_required: total.sub(deposit)?.floor_at_zero(),
    })
}

#[cfg(test)]
mod tests {
    use super::purchase_costs;
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, PropertyCostParameters};

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    fn costs() -> PropertyCostParameters {
        PropertyCostParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn buy(price: i64, land: Bundesland, deposit: i64, agent_percent: i64) -> super::PurchaseCosts {
        purchase_costs(
            euro(price),
            land,
            euro(deposit),
            Rate::from_percent_millis(agent_percent).unwrap(),
            &costs(),
        )
        .expect("computes")
    }

    /// A worked purchase, every line checked by hand. 400 000 € in Nordrhein-Westfalen:
    /// 6,5 % transfer tax is 26 000 €, 2 % notary and registry is 8 000 €, and a 3,57 % agent
    /// share is 14 280 € — 48 280 € on top of the price.
    #[test]
    fn a_worked_purchase_adds_up() {
        let result = buy(400_000, Bundesland::NordrheinWestfalen, 80_000, 3_570);
        assert_eq!(result.transfer_tax, euro(26_000));
        assert_eq!(result.notary_and_registry, euro(8_000));
        assert_eq!(result.agent_commission, euro(14_280));
        assert_eq!(result.incidental_total, euro(48_280));
        assert_eq!(result.total, euro(448_280));
        assert_eq!(result.loan_required, euro(368_280));
    }

    /// The finding worth putting in front of a household: incidental costs eat the deposit.
    ///
    /// 80 000 € against a 400 000 € house looks like 20 % down. After costs the borrower is
    /// putting 7,9 % of the price into equity and borrowing 92 % of it — which is a different
    /// mortgage, at a different rate, and often not one a bank will write.
    #[test]
    fn the_incidental_costs_eat_the_deposit() {
        let result = buy(400_000, Bundesland::NordrheinWestfalen, 80_000, 3_570);

        // Nominally a fifth down …
        assert_eq!(result.deposit_against_price().unwrap().ppm(), 200_000);
        // … but the loan is 92 % of the price rather than 80 %.
        let loan_share = result.loan_required.cents() * 1_000_000 / result.price.cents();
        assert!(
            (915_000..925_000).contains(&loan_share),
            "the loan came to {loan_share} ppm of the price"
        );
    }

    /// The state's choice moves the bill by twelve thousand euro on the same house.
    #[test]
    fn the_state_moves_the_total_by_twelve_thousand() {
        let cheap = buy(400_000, Bundesland::Bayern, 80_000, 0);
        let dear = buy(400_000, Bundesland::NordrheinWestfalen, 80_000, 0);
        assert_eq!(dear.total.sub(cheap.total).unwrap(), euro(12_000));
        assert_eq!(cheap.transfer_tax, euro(14_000));
        assert_eq!(dear.transfer_tax, euro(26_000));
    }

    /// Incidentals come to roughly a tenth of the price with an agent and a twelfth without,
    /// which is the range a household should expect.
    #[test]
    fn the_incidental_share_lands_where_the_market_quotes_it() {
        let with_agent = buy(400_000, Bundesland::NordrheinWestfalen, 0, 3_570)
            .incidental_rate()
            .unwrap();
        let without = buy(400_000, Bundesland::NordrheinWestfalen, 0, 0)
            .incidental_rate()
            .unwrap();
        assert_eq!(without.ppm(), 85_000, "6,5 % + 2 %");
        assert_eq!(with_agent.ppm(), 120_700, "and 3,57 % more with an agent");
    }

    /// No agent means no commission, and nothing else changes.
    #[test]
    fn a_private_sale_has_no_commission() {
        let result = buy(300_000, Bundesland::Bayern, 60_000, 0);
        assert_eq!(result.agent_commission, Money::ZERO);
        assert_eq!(
            result.incidental_total,
            result.transfer_tax.add(result.notary_and_registry).unwrap()
        );
    }

    /// A deposit covering everything leaves nothing to borrow, and one larger than the
    /// purchase is capped rather than producing a negative loan.
    #[test]
    fn a_deposit_covering_the_purchase_leaves_no_loan() {
        let exact = buy(300_000, Bundesland::Bayern, 316_500, 0);
        assert_eq!(exact.loan_required, Money::ZERO);

        let excessive = buy(300_000, Bundesland::Bayern, 900_000, 0);
        assert_eq!(excessive.loan_required, Money::ZERO);
        assert_eq!(
            excessive.deposit, excessive.total,
            "capped at what is needed"
        );
    }

    /// Every part must reconcile with the totals.
    #[test]
    fn the_parts_reconcile() {
        for land in Bundesland::ALL {
            let result = buy(250_000, land, 50_000, 2_000);
            assert_eq!(
                result.incidental_total,
                result
                    .transfer_tax
                    .add(result.notary_and_registry)
                    .unwrap()
                    .add(result.agent_commission)
                    .unwrap()
            );
            assert_eq!(
                result.total,
                result.price.add(result.incidental_total).unwrap()
            );
            assert_eq!(
                result.total,
                result.deposit.add(result.loan_required).unwrap()
            );
        }
    }

    #[test]
    fn a_zero_price_costs_nothing_rather_than_failing() {
        let result = buy(0, Bundesland::Bayern, 0, 3_570);
        assert_eq!(result.total, Money::ZERO);
        assert_eq!(result.incidental_rate().unwrap(), Rate::ZERO);
        assert_eq!(result.deposit_against_price().unwrap(), Rate::ZERO);
    }
}
