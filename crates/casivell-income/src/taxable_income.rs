//! The § 2 EStG chain from gross pay to taxable income.

use casivell_core::{Money, MoneyError};
use casivell_lawdata::DeductionParameters;

use crate::vorsorge::{Contributions, Vorsorgeaufwendungen, vorsorgeaufwendungen};

/// What an employee earned and paid over a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Employee {
    /// Bruttoarbeitslohn for the year.
    pub gross_annual: Money,
    /// Actual Werbungskosten under § 9, if they exceed the Pauschbetrag.
    ///
    /// The Pauschbetrag is a *floor*, not a cap, so the larger of the two applies. Passing
    /// zero uses the Pauschbetrag, which is what the overwhelming majority of employees get.
    pub work_expenses: Money,
    /// Provision expenses actually paid.
    pub contributions: Contributions,
    /// Church tax paid during the year, deductible under § 10 Abs. 1 Nr. 4 EStG.
    ///
    /// # A circularity the statute creates and this crate does not resolve
    ///
    /// Church tax is levied on the income tax, and is itself deductible in computing the
    /// income tax. The true figures are the fixed point of that relationship. German practice
    /// deducts the church tax actually *paid* in the calendar year — which is the prior
    /// year's withholding, not this year's liability — so the circularity is broken by the
    /// calendar rather than by iteration.
    ///
    /// This field therefore takes what was paid, and does not attempt to solve the fixed
    /// point. A caller passing this year's computed church tax would be modelling something
    /// the statute does not do.
    pub church_tax_paid: Money,
    /// Other Sonderausgaben under §§ 10–10b: donations, maintenance payments, training costs.
    pub other_special_expenses: Money,
    /// Tax-free wage-replacement benefits received in the year, § 32b Abs. 1 Nr. 1 EStG.
    ///
    /// Elterngeld, Arbeitslosengeld, Kurzarbeitergeld, Krankengeld. **Not** taxable income
    /// and deliberately not added to any total here — but it raises the rate on everything
    /// else through the Progressionsvorbehalt, so it is carried through to the assessment.
    /// Held on the employee rather than passed to [`crate::assess`] separately because it is
    /// a fact about the person's year, like their salary.
    pub wage_replacement_benefits: Money,
    /// Children entitled to a Kinderfreibetrag, counted in whole children for this taxpayer.
    ///
    /// A parent assessed individually normally holds half of each child's allowance, so the
    /// halving is the caller's to apply through [`crate::assessment::ChildRelief`] rather
    /// than being guessed here.
    pub children: u8,
}

/// The § 2 EStG chain, with every stage exposed.
///
/// Every intermediate is reported because reconciling a Steuerbescheid means finding *which*
/// stage diverged, and a single output figure makes that impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaxableIncome {
    /// Bruttoarbeitslohn.
    pub gross: Money,
    /// The Werbungskosten actually deducted — the larger of the actual and the Pauschbetrag.
    pub work_expenses_deducted: Money,
    /// Whether the Pauschbetrag was used rather than actual expenses.
    ///
    /// Worth surfacing: a user who has kept receipts wants to know whether they made any
    /// difference, and for most employees they do not.
    pub work_expenses_lump_sum_used: bool,

    /// Einkünfte aus nichtselbständiger Arbeit, § 19 EStG.
    pub employment_income: Money,
    /// Gesamtbetrag der Einkünfte. Equal to the above for an employee with no other income.
    pub total_income: Money,

    /// The Vorsorgeaufwendungen deduction, with its own parts.
    pub provision: Vorsorgeaufwendungen,
    /// The *other* Sonderausgaben deducted — the larger of the actual and the § 10c
    /// Pauschbetrag.
    pub other_special_expenses_deducted: Money,
    /// Whether the § 10c Pauschbetrag was used.
    pub special_expenses_lump_sum_used: bool,

    /// Einkommen: total income less all Sonderausgaben.
    ///
    /// This is the figure the Kinderfreibetrag is subtracted from, and therefore the base the
    /// Günstigerprüfung compares at.
    pub income: Money,

    /// The § 32b benefits carried through from [`Employee`], untaxed but rate-raising.
    ///
    /// Present on the *output* as well as the input so the assessment cannot be run without
    /// them by accident: a caller that has a `TaxableIncome` has everything § 32b needs.
    pub wage_replacement_benefits: Money,
    /// Bruttoarbeitslohn, carried through because § 32b Abs. 2 Nr. 1 needs it to decide how
    /// much of the Arbeitnehmer-Pauschbetrag the employment income already absorbed.
    pub employment_gross: Money,
}

/// Runs the § 2 EStG chain down to *Einkommen*.
///
/// The final step — subtracting the Kinderfreibeträge to reach the taxable income — is not
/// done here, because whether to subtract them at all depends on a comparison against the
/// Kindergeld received and therefore on the tax itself. See [`crate::assessment::assess`].
///
/// # Errors
///
/// [`MoneyError`] on a domain violation.
pub fn taxable_income(
    employee: &Employee,
    deductions: &DeductionParameters,
) -> Result<TaxableIncome, MoneyError> {
    // § 9a: the Pauschbetrag is a floor, so the larger of the two applies.
    let lump_sum = deductions.employee_lump_sum;
    let work_expenses_deducted = employee.work_expenses.max(lump_sum);
    let work_expenses_lump_sum_used = employee.work_expenses <= lump_sum;

    // § 19: employment income. Floored at zero — negative employment income is not a thing,
    // and Werbungskosten beyond the salary are a loss question under § 10d rather than a
    // negative Einkünfte here.
    let employment_income = employee
        .gross_annual
        .sub(work_expenses_deducted)?
        .floor_at_zero();
    let total_income = employment_income;

    let provision = vorsorgeaufwendungen(&employee.contributions, deductions)?;

    // § 10c: the Pauschbetrag covers the *other* Sonderausgaben, not the provision expenses,
    // which are deducted separately and in full.
    let actual_other = employee
        .church_tax_paid
        .add(employee.other_special_expenses)?;
    let other_lump_sum = deductions.special_expenses_lump_sum;
    let other_special_expenses_deducted = actual_other.max(other_lump_sum);
    let special_expenses_lump_sum_used = actual_other <= other_lump_sum;

    let income = total_income
        .sub(provision.total)?
        .sub(other_special_expenses_deducted)?
        .floor_at_zero();

    Ok(TaxableIncome {
        gross: employee.gross_annual,
        work_expenses_deducted,
        work_expenses_lump_sum_used,
        employment_income,
        total_income,
        provision,
        other_special_expenses_deducted,
        special_expenses_lump_sum_used,
        income,
        wage_replacement_benefits: employee.wage_replacement_benefits.floor_at_zero(),
        employment_gross: employee.gross_annual.floor_at_zero(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Employee, taxable_income};
    use crate::vorsorge::Contributions;
    use casivell_core::{Money, TaxYear};
    use casivell_lawdata::DeductionParameters;

    fn deductions() -> DeductionParameters {
        DeductionParameters::for_year(TaxYear::new(2026).unwrap()).unwrap()
    }

    fn euro(amount: i64) -> Money {
        Money::from_euro(amount).unwrap()
    }

    /// A childless employee on 54 000 EUR, with the contributions the 2026 rates produce.
    fn employee() -> Employee {
        Employee {
            gross_annual: euro(54_000),
            work_expenses: Money::ZERO,
            contributions: Contributions {
                pension_employee: euro(5_022),
                pension_employer: euro(5_022),
                retirement_voluntary: Money::ZERO,
                health_general: euro(3_942),
                health_supplementary: euro(783),
                care: euro(1_296),
                other_provision: euro(702),
            },
            church_tax_paid: Money::ZERO,
            other_special_expenses: Money::ZERO,
            wage_replacement_benefits: Money::ZERO,
            children: 0,
        }
    }

    /// The whole chain on a concrete case, stage by stage. Each figure is checkable by hand
    /// against the statute, which is the point of exposing them.
    #[test]
    fn the_chain_computes_each_stage() {
        let result = taxable_income(&employee(), &deductions()).expect("computes");

        // § 9a: the Pauschbetrag, since no actual expenses were claimed.
        assert_eq!(result.work_expenses_deducted, euro(1_230));
        assert!(result.work_expenses_lump_sum_used);
        // § 19: 54 000 − 1 230.
        assert_eq!(result.employment_income, euro(52_770));

        // § 10 Abs. 1 Nr. 2: the employee's own pension contribution.
        assert_eq!(result.provision.retirement, euro(5_022));
        // Nr. 3: 3 942 × 0.96 + 783 + 1 296.
        assert_eq!(
            result.provision.other,
            Money::from_euro_cents(5_863, 32).unwrap()
        );

        // § 10c: the 36 EUR Pauschbetrag, since nothing else was claimed.
        assert_eq!(result.other_special_expenses_deducted, euro(36));
        assert!(result.special_expenses_lump_sum_used);

        // Einkommen: 52 770 − 5 022 − 5 863.32 − 36 = 41 848.68.
        assert_eq!(result.income, Money::from_euro_cents(41_848, 68).unwrap());
    }

    /// Actual Werbungskosten replace the Pauschbetrag only when they exceed it — it is a
    /// floor, not a cap. Getting that backwards would deny the deduction to everyone who
    /// spends less than 1 230 EUR.
    #[test]
    fn the_work_expenses_lump_sum_is_a_floor() {
        let mut modest = employee();
        modest.work_expenses = euro(400);
        let result = taxable_income(&modest, &deductions()).expect("computes");
        assert_eq!(result.work_expenses_deducted, euro(1_230));
        assert!(result.work_expenses_lump_sum_used);

        let mut substantial = employee();
        substantial.work_expenses = euro(3_000);
        let result = taxable_income(&substantial, &deductions()).expect("computes");
        assert_eq!(result.work_expenses_deducted, euro(3_000));
        assert!(!result.work_expenses_lump_sum_used);
    }

    /// The § 10c Pauschbetrag behaves the same way, and church tax paid counts toward it.
    #[test]
    fn the_special_expenses_lump_sum_is_also_a_floor() {
        let mut churchgoer = employee();
        churchgoer.church_tax_paid = euro(700);
        let result = taxable_income(&churchgoer, &deductions()).expect("computes");
        assert_eq!(result.other_special_expenses_deducted, euro(700));
        assert!(!result.special_expenses_lump_sum_used);

        // A trivial donation stays below the Pauschbetrag, which then applies.
        let mut trivial = employee();
        trivial.other_special_expenses = euro(20);
        let result = taxable_income(&trivial, &deductions()).expect("computes");
        assert_eq!(result.other_special_expenses_deducted, euro(36));
    }

    /// Taxable income must always be below gross, and the ordering of the stages must hold.
    /// A stage out of order would be invisible in the final figure alone.
    #[test]
    fn the_stages_decrease_monotonically() {
        let mut gross = 12_000_i64;
        while gross <= 200_000 {
            let mut e = employee();
            e.gross_annual = euro(gross);
            let r = taxable_income(&e, &deductions()).expect("computes");

            assert!(
                r.employment_income <= r.gross,
                "income exceeded gross at {gross}"
            );
            assert_eq!(r.total_income, r.employment_income);
            assert!(
                r.income <= r.total_income,
                "Einkommen exceeded GdE at {gross}"
            );
            assert!(!r.income.is_negative(), "negative Einkommen at {gross}");
            gross = gross.saturating_add(3_700);
        }
    }

    /// Einkommen rises with gross pay. A band where earning more lowered taxable income would
    /// be a serious defect in the caps.
    #[test]
    fn taxable_income_is_monotonic_in_gross_pay() {
        let mut previous = Money::ZERO;
        let mut gross = 0_i64;
        while gross <= 150_000 {
            let mut e = employee();
            e.gross_annual = euro(gross);
            let income = taxable_income(&e, &deductions()).expect("computes").income;
            assert!(income >= previous, "Einkommen fell at {gross}");
            previous = income;
            gross = gross.saturating_add(2_500);
        }
    }

    /// A very small salary cannot produce a negative taxable income, however large the
    /// deductions.
    #[test]
    fn a_tiny_salary_floors_at_zero() {
        let mut tiny = employee();
        tiny.gross_annual = euro(800);
        let result = taxable_income(&tiny, &deductions()).expect("computes");
        assert_eq!(result.employment_income, Money::ZERO);
        assert_eq!(result.income, Money::ZERO);
    }

    /// Deductions must actually reduce the taxable income — a chain that computed them and
    /// then ignored them would pass every monotonicity test.
    #[test]
    fn the_deductions_reduce_the_taxable_income_materially() {
        let result = taxable_income(&employee(), &deductions()).expect("computes");
        let reduction = result.gross.sub(result.income).unwrap();
        // Roughly 12 150 EUR on a 54 000 EUR salary: the Pauschbetrag, the pension
        // contribution, health and care cover, and the 36 EUR Pauschbetrag.
        assert!(
            reduction > euro(11_000) && reduction < euro(13_000),
            "the deductions came to {} cents",
            reduction.cents()
        );
    }
}
