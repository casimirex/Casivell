//! Net pay: gross less withholding and social insurance.
//!
//! This is the composition the whole engine has been building toward, and it is
//! deliberately thin. All the difficulty lives in the two things it composes —
//! [`crate::withholding`] for Lohnsteuer and `casivell_social` for contributions —
//! and neither knows about the other. The only thing this module contributes is the
//! subtraction, and the guarantee that both halves describe the same person.
//!
//! That guarantee is the reason [`monthly_net`] takes one [`Employment`] rather
//! than a payroll input and a contribution input. The care-insurance flags the
//! Vorsorgepauschale needs (`PVS`, `PVZ`, `PVA`) are derived from the same
//! [`casivell_social::Insured`] the contributions are computed from, so a person
//! cannot be childless for tax and a parent for insurance.

use casivell_core::{Money, MoneyError};
use casivell_social::{SocialContributions, contributions};

use crate::withholding::{Employment, PayPeriod, PayrollLaw, Withholding, withhold};

/// A month's pay, fully decomposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetPay {
    /// Gross pay for the month.
    pub gross: Money,
    /// Lohnsteuer withheld.
    pub income_tax: Money,
    /// Solidaritätszuschlag withheld.
    pub solidarity_surcharge: Money,
    /// Church tax withheld.
    pub church_tax: Money,
    /// The employee's social insurance contributions.
    pub employee_contributions: Money,
    /// What reaches the employee's account.
    pub net: Money,
    /// The employer's social insurance contributions, which never touch the
    /// employee's payslip but are part of the true cost of the employment.
    pub employer_contributions: Money,
    /// Total cost to the employer: gross plus their contributions.
    ///
    /// Reported because "what does a raise cost?" and "what am I worth?" are
    /// questions a household planner should be able to answer, and the answer is
    /// roughly 20 % above gross.
    pub employer_cost: Money,
    /// The full withholding calculation, for inspection.
    pub withholding: Withholding,
    /// The full contribution calculation, for inspection.
    pub contributions: SocialContributions,
}

impl NetPay {
    /// Everything deducted from gross pay.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn total_deductions(&self) -> Result<Money, MoneyError> {
        let taxes = match self.income_tax.add(self.solidarity_surcharge) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let with_church = match taxes.add(self.church_tax) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        with_church.add(self.employee_contributions)
    }
}

/// Computes one month's net pay from gross.
///
/// # Errors
///
/// [`MoneyError`] if an intermediate leaves the representable domain.
pub fn monthly_net(
    monthly_gross: Money,
    employment: &Employment,
    law: &PayrollLaw,
) -> Result<NetPay, MoneyError> {
    let gross = monthly_gross.floor_at_zero();

    let withholding = withhold(gross, PayPeriod::Month, employment, law)?;
    let contributions = contributions(gross, &law.social, &employment.insured)?;

    let employee_contributions = contributions.employee_total()?;
    let employer_contributions = contributions.employer_total()?;

    let deductions = withholding
        .income_tax
        .add(withholding.solidarity_surcharge)?
        .add(withholding.church_tax)?
        .add(employee_contributions)?;

    Ok(NetPay {
        gross,
        income_tax: withholding.income_tax,
        solidarity_surcharge: withholding.solidarity_surcharge,
        church_tax: withholding.church_tax,
        employee_contributions,
        // Net can in principle go negative only for contrived inputs; the
        // subtraction is left unclamped so that such a case is visible rather than
        // silently floored at zero.
        net: gross.sub(deductions)?,
        employer_contributions,
        employer_cost: gross.add(employer_contributions)?,
        contributions,
        withholding,
    })
}

#[cfg(test)]
mod tests {
    use super::monthly_net;
    use crate::withholding::{Employment, HealthCover, PayrollLaw};
    use casivell_core::{Money, Rate, TaxYear};
    use casivell_lawdata::{Bundesland, TaxClass};
    use casivell_social::Insured;

    fn law() -> PayrollLaw {
        PayrollLaw::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn childless_employment(class: TaxClass) -> Employment {
        let insured = Insured::new(30, false, 0, Bundesland::NordrheinWestfalen, None).unwrap();
        Employment::new(
            insured,
            class,
            0,
            HealthCover::Statutory {
                supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
            },
            None,
        )
        .unwrap()
    }

    /// The decomposition must be exact: net plus every deduction equals gross.
    /// If this fails, money is being created or destroyed.
    #[test]
    fn the_decomposition_is_exact() {
        let mut gross_euro = 0_i64;
        while gross_euro <= 12_000 {
            let gross = Money::from_euro(gross_euro).unwrap();
            let pay = monthly_net(gross, &childless_employment(TaxClass::Class1), &law())
                .unwrap_or_else(|e| panic!("failed at {gross_euro}: {e}"));
            let recomposed = pay.net.add(pay.total_deductions().unwrap()).unwrap();
            assert_eq!(
                recomposed, pay.gross,
                "net + deductions != gross at {gross_euro} EUR"
            );
            gross_euro = gross_euro.saturating_add(163);
        }
    }

    /// Employer cost is gross plus the employer's contributions, and exceeds gross
    /// for any positive salary.
    #[test]
    fn employer_cost_exceeds_gross() {
        let pay = monthly_net(
            Money::from_euro(4_000).unwrap(),
            &childless_employment(TaxClass::Class1),
            &law(),
        )
        .unwrap();
        assert!(pay.employer_cost > pay.gross);
        assert_eq!(
            pay.employer_cost,
            pay.gross.add(pay.employer_contributions).unwrap()
        );
    }

    /// Net pay rises monotonically with gross. A band where earning more reduced
    /// take-home would be a serious defect, and the class V/VI formula and the
    /// Vorsorgepauschale's maximum both create places one could hide.
    #[test]
    fn net_pay_is_monotonic_in_gross() {
        for class in TaxClass::ALL {
            let employment = childless_employment(class);
            let mut previous = Money::ZERO;
            let mut gross_euro = 0_i64;
            while gross_euro <= 15_000 {
                let gross = Money::from_euro(gross_euro).unwrap();
                let pay = monthly_net(gross, &employment, &law()).unwrap();
                assert!(
                    pay.net >= previous,
                    "{class:?}: net fell at {gross_euro} EUR, from {} to {} cents",
                    previous.cents(),
                    pay.net.cents()
                );
                previous = pay.net;
                gross_euro = gross_euro.saturating_add(71);
            }
        }
    }

    /// Net is always less than gross once anything is owed, and never negative for
    /// a plausible salary.
    #[test]
    fn net_stays_between_zero_and_gross() {
        for class in TaxClass::ALL {
            let employment = childless_employment(class);
            let mut gross_euro = 0_i64;
            while gross_euro <= 15_000 {
                let gross = Money::from_euro(gross_euro).unwrap();
                let pay = monthly_net(gross, &employment, &law()).unwrap();
                assert!(
                    !pay.net.is_negative(),
                    "{class:?}: negative net at {gross_euro}"
                );
                assert!(
                    pay.net <= pay.gross,
                    "{class:?}: net exceeded gross at {gross_euro}"
                );
                gross_euro = gross_euro.saturating_add(211);
            }
        }
    }

    #[test]
    fn no_pay_means_no_deductions_and_no_net() {
        let pay =
            monthly_net(Money::ZERO, &childless_employment(TaxClass::Class1), &law()).unwrap();
        assert_eq!(pay.net, Money::ZERO);
        assert_eq!(pay.total_deductions().unwrap(), Money::ZERO);
    }

    /// Class III takes home more than class I on the same salary, and class V less.
    /// This is the ordering the whole III/V versus IV/IV decision turns on.
    #[test]
    fn the_tax_classes_order_net_pay_as_expected() {
        let gross = Money::from_euro(4_000).unwrap();
        let net_for = |class| {
            monthly_net(gross, &childless_employment(class), &law())
                .unwrap()
                .net
        };
        let class1 = net_for(TaxClass::Class1);
        let class3 = net_for(TaxClass::Class3);
        let class4 = net_for(TaxClass::Class4);
        let class5 = net_for(TaxClass::Class5);
        let class6 = net_for(TaxClass::Class6);

        assert!(class3 > class1, "class III should beat class I");
        assert_eq!(class1, class4, "classes I and IV withhold identically");
        assert!(class5 < class1, "class V should be worse than class I");
        assert!(
            class6 <= class5,
            "class VI should be no better than class V"
        );
    }

    /// Church membership reduces net pay, and by more in a 9 % state than an 8 %
    /// one.
    #[test]
    fn church_membership_reduces_net_pay_by_the_state_rate() {
        let gross = Money::from_euro(5_000).unwrap();
        let insured = Insured::new(30, false, 0, Bundesland::Bayern, None).unwrap();
        let statutory = HealthCover::Statutory {
            supplementary_rate: Rate::from_percent_millis(2_900).unwrap(),
        };

        let secular = Employment::new(insured, TaxClass::Class1, 0, statutory, None).unwrap();
        let bavarian = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            statutory,
            Some(Bundesland::Bayern),
        )
        .unwrap();
        let rhenish = Employment::new(
            insured,
            TaxClass::Class1,
            0,
            statutory,
            Some(Bundesland::NordrheinWestfalen),
        )
        .unwrap();

        let net = |e: &Employment| monthly_net(gross, e, &law()).unwrap();
        assert!(net(&bavarian).net < net(&secular).net);
        // 9 % costs more than 8 %.
        assert!(net(&rhenish).net < net(&bavarian).net);
        assert_eq!(net(&secular).church_tax, Money::ZERO);
    }
}
