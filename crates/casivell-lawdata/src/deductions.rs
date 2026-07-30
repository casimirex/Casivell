//! Parameters for determining taxable income: § 9a, § 10, § 32 and § 66 EStG.
//!
//! # The chain these figures serve
//!
//! § 2 EStG builds the *zu versteuerndes Einkommen* in stages, and each stage has its own
//! statutory deductions:
//!
//! ```text
//!   Bruttoarbeitslohn
//!   − Werbungskosten (§ 9), at least the Pauschbetrag (§ 9a)
//!   = Einkünfte aus nichtselbständiger Arbeit (§ 19)
//!   = Gesamtbetrag der Einkünfte
//!   − Sonderausgaben (§§ 10–10c), of which Vorsorgeaufwendungen dominate
//!   = Einkommen
//!   − Freibeträge für Kinder (§ 32 Abs. 6), if the Günstigerprüfung favours them
//!   = zu versteuerndes Einkommen
//! ```
//!
//! # The Altersvorsorge cap is derived, not transcribed
//!
//! § 10 Abs. 3 Satz 1 sets it as *the maximum contribution to the miners' pension
//! insurance* — a figure that follows from that scheme's own ceiling and rate rather than
//! being stated as an amount. For 2026 that is `124 800 € × 24.7 % = 30 825.60 €`, rounded
//! up to `30 826 €`.
//!
//! Both components are stored and the cap is asserted against the published figure, so the
//! derivation is checked rather than assumed. It also means the cap moves correctly when
//! the miners' scheme's ceiling changes, which is a different series from the general
//! pension ceiling.
//!
//! # Two details that are easy to get wrong
//!
//! **The employee's cap for *other* provision expenses is 1 900 €, not 2 800 €.** The higher
//! figure applies to someone bearing their own health costs entirely; an employee receives
//! a tax-free employer contribution under § 3 Nr. 62 EStG and falls under the reduced cap
//! (§ 10 Abs. 4 Satz 2). Using 2 800 € would overstate the deduction for every employee.
//!
//! **The 4 % Krankengeld reduction applies to the general rate only.** § 10 Abs. 1 Nr. 3
//! Buchst. a Satz 4 reduces a health contribution by 4 % where it can give rise to a
//! Krankengeld entitlement, because that is not basic cover. It does **not** apply to the
//! Zusatzbeitrag. Applying it to the whole contribution would understate the deduction.

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// Parameters for the deductions between gross pay and taxable income.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeductionParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,

    /// Arbeitnehmer-Pauschbetrag, § 9a Satz 1 Nr. 1 Buchst. a EStG.
    ///
    /// A floor rather than a cap: actual Werbungskosten are deductible instead when they
    /// exceed it.
    pub employee_lump_sum: Money,
    /// Sonderausgaben-Pauschbetrag, § 10c EStG.
    ///
    /// Applies to the *other* Sonderausgaben — church tax, donations — and not to
    /// Vorsorgeaufwendungen, which are deducted separately and in full.
    pub special_expenses_lump_sum: Money,

    /// Annual contribution ceiling of the miners' pension insurance (West), which
    /// § 10 Abs. 3 Satz 1 uses to set the Altersvorsorge cap.
    pub miners_pension_ceiling_annual: Money,
    /// Contribution rate of the miners' pension insurance.
    pub miners_pension_rate: Rate,

    /// § 10 Abs. 4 Satz 1: the cap on other provision expenses for someone bearing their
    /// own health costs.
    pub other_provision_cap: Money,
    /// § 10 Abs. 4 Satz 2: the reduced cap that applies to an employee.
    ///
    /// See the module documentation — this is the one that applies to anyone receiving a
    /// tax-free employer contribution, which is every ordinary employee.
    pub other_provision_cap_employee: Money,
    /// § 10 Abs. 1 Nr. 3 Buchst. a Satz 4: the reduction for a Krankengeld entitlement.
    pub sick_pay_reduction: Rate,

    /// § 32 Abs. 6 Satz 1: the sächlicher Kinderfreibetrag, for both parents together.
    pub child_allowance_material: Money,
    /// § 32 Abs. 6 Satz 1: the BEA-Freibetrag for care, upbringing and education, for both
    /// parents together.
    pub child_allowance_care: Money,
    /// § 66 Abs. 1 EStG: monthly Kindergeld per child.
    ///
    /// The Günstigerprüfung of § 31 EStG compares the tax saved by the allowances against
    /// the Kindergeld received, so both are needed together.
    pub child_benefit_monthly: Money,

    /// § 20 Abs. 9 EStG: the Sparer-Pauschbetrag, per person.
    ///
    /// Unchanged since 2023. Doubled for a joint assessment, and — unlike most allowances —
    /// it is a *flat* exemption on capital income rather than a floor: capital income below it
    /// is untaxed, and only the excess attracts the flat rate.
    pub saver_allowance: Money,
    /// § 32d Abs. 1 EStG: the Abgeltungsteuer rate on capital income, 25 %.
    ///
    /// A flat rate rather than the progressive tariff, which is why § 32d Abs. 6 exists: a
    /// taxpayer whose marginal rate is below 25 % may elect the ordinary tariff instead.
    pub capital_income_rate: Rate,

    /// Citation.
    pub provenance: Provenance,
}

impl DeductionParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2026 => Ok(DEDUCTIONS_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The combined Kinderfreibetrag for both parents: material plus BEA.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] if the sum leaves the representable domain.
    pub const fn child_allowance_total(&self) -> Result<Money, MoneyError> {
        self.child_allowance_material.add(self.child_allowance_care)
    }

    /// § 10 Abs. 3 Satz 1: the Altersvorsorge cap, derived from the miners' scheme.
    ///
    /// Rounded **up** to a whole euro, which is what produces the published 30 826 € from
    /// an exact 30 825.60 €.
    ///
    /// # Errors
    ///
    /// [`MoneyError`] on a domain violation.
    pub const fn retirement_provision_cap(&self) -> Result<Money, MoneyError> {
        let exact = match self
            .miners_pension_ceiling_annual
            .mul_rate(self.miners_pension_rate, casivell_core::Rounding::Ceiling)
        {
            Ok(value) => value,
            Err(e) => return Err(e),
        };
        exact.ceil_to_euro()
    }

    /// Annual Kindergeld per child.
    ///
    /// # Errors
    ///
    /// [`MoneyError::Overflow`] on a domain violation.
    pub const fn child_benefit_annual(&self) -> Result<Money, MoneyError> {
        self.child_benefit_monthly.mul_int(12)
    }
}

const fn euro(whole: i64) -> Money {
    match Money::from_euro(whole) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

const fn year(value: u16) -> TaxYear {
    match TaxYear::new(value) {
        Ok(y) => y,
        Err(_) => TaxYear::LAST_VERIFIED,
    }
}

/// Deduction parameters for 2026.
const DEDUCTIONS_2026: DeductionParameters = DeductionParameters {
    year: year(2026),

    employee_lump_sum: euro(1_230),
    special_expenses_lump_sum: euro(36),

    miners_pension_ceiling_annual: euro(124_800),
    miners_pension_rate: pct_milli(24_700),

    other_provision_cap: euro(2_800),
    other_provision_cap_employee: euro(1_900),
    sick_pay_reduction: pct_milli(4_000),

    child_allowance_material: euro(6_828),
    child_allowance_care: euro(2_928),
    child_benefit_monthly: euro(259),

    saver_allowance: euro(1_000),
    capital_income_rate: pct_milli(25_000),

    provenance: Provenance::new(
        "§ 9a, § 10 Abs. 1 Nr. 2-3a, § 10 Abs. 3-4, § 10c, § 20 Abs. 9, § 32 Abs. 6, § 32d, § 66 EStG",
        "https://www.gesetze-im-internet.de/estg/__10.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{DEDUCTIONS_2026, DeductionParameters};
    use casivell_core::{Money, Rate, TaxYear};

    #[test]
    fn no_amount_or_rate_in_the_table_is_accidentally_zero() {
        let p = DEDUCTIONS_2026;
        let amounts = [
            ("ANP", p.employee_lump_sum),
            ("SAP", p.special_expenses_lump_sum),
            ("miners' ceiling", p.miners_pension_ceiling_annual),
            ("cap", p.other_provision_cap),
            ("employee cap", p.other_provision_cap_employee),
            ("Kinderfreibetrag", p.child_allowance_material),
            ("BEA", p.child_allowance_care),
            ("Kindergeld", p.child_benefit_monthly),
        ];
        for (name, amount) in amounts {
            assert!(
                !amount.is_zero() && !amount.is_negative(),
                "{name} is {} cents, so the literal was rejected",
                amount.cents()
            );
        }
        assert!(!p.miners_pension_rate.is_zero());
        assert!(!p.sick_pay_reduction.is_zero());
    }

    /// The Altersvorsorge cap is a *derived* figure, and the derivation must reproduce the
    /// published one: 124 800 € × 24.7 % = 30 825.60 €, rounded up to 30 826 €.
    ///
    /// Checking the derivation rather than transcribing the result means the cap stays
    /// correct when the miners' scheme's ceiling changes — a different series from the
    /// general pension ceiling, and one it would be easy to conflate.
    #[test]
    fn the_retirement_provision_cap_matches_the_published_figure() {
        let cap = DEDUCTIONS_2026
            .retirement_provision_cap()
            .expect("in domain");
        assert_eq!(
            cap,
            Money::from_euro(30_826).expect("valid"),
            "the derived cap should be the published 30 826 EUR"
        );
        // And the exact product is 30 825.60, so the rounding direction matters.
        let exact = DEDUCTIONS_2026
            .miners_pension_ceiling_annual
            .mul_rate(
                DEDUCTIONS_2026.miners_pension_rate,
                casivell_core::Rounding::Floor,
            )
            .expect("in domain");
        assert_eq!(exact.cents(), 3_082_560);
    }

    /// The employee's cap must be the lower of the two. Using the general 2 800 EUR figure
    /// would overstate every employee's deduction, which is why they are stored separately
    /// and the ordering is asserted.
    #[test]
    fn the_employee_cap_is_the_lower_one() {
        let p = DEDUCTIONS_2026;
        assert!(p.other_provision_cap_employee < p.other_provision_cap);
        assert_eq!(
            p.other_provision_cap_employee,
            Money::from_euro(1_900).expect("valid")
        );
        assert_eq!(
            p.other_provision_cap,
            Money::from_euro(2_800).expect("valid")
        );
    }

    /// The combined Kinderfreibetrag must equal the figure the Programmablaufplan uses, so
    /// the withholding path and the assessment path cannot disagree about a family.
    ///
    /// Two independently transcribed sources agreeing is the strongest consistency check
    /// available for statutory data.
    #[test]
    fn the_child_allowance_agrees_with_the_payroll_table() {
        use crate::payroll::PayrollParameters;

        let deductions = DEDUCTIONS_2026;
        let payroll = PayrollParameters::for_year(TaxYear::new(2026).unwrap()).expect("enacted");

        assert_eq!(
            deductions.child_allowance_total().expect("in domain"),
            payroll.child_allowance_full,
            "§ 32 Abs. 6 and the PAP disagree about the Kinderfreibetrag"
        );
        // And the split is the published 6 828 + 2 928.
        assert_eq!(
            deductions.child_allowance_material,
            Money::from_euro(6_828).expect("valid")
        );
        assert_eq!(
            deductions.child_allowance_care,
            Money::from_euro(2_928).expect("valid")
        );
    }

    /// The Arbeitnehmer- and Sonderausgaben-Pauschbeträge are the same statutory figures the
    /// PAP uses, so they must match there too.
    #[test]
    fn the_lump_sums_agree_with_the_payroll_table() {
        use crate::payroll::PayrollParameters;

        let payroll = PayrollParameters::for_year(TaxYear::new(2026).unwrap()).expect("enacted");
        assert_eq!(DEDUCTIONS_2026.employee_lump_sum, payroll.employee_lump_sum);
        assert_eq!(
            DEDUCTIONS_2026.special_expenses_lump_sum,
            payroll.special_expenses_lump_sum
        );
    }

    #[test]
    fn annual_child_benefit_is_twelve_monthly_payments() {
        assert_eq!(
            DEDUCTIONS_2026.child_benefit_annual().expect("in domain"),
            Money::from_euro(259 * 12).expect("valid")
        );
    }

    /// § 20 Abs. 9 and § 32d Abs. 1: the Sparer-Pauschbetrag and the flat rate.
    #[test]
    fn the_capital_income_figures_match_the_statute() {
        assert_eq!(
            DEDUCTIONS_2026.saver_allowance,
            Money::from_euro(1_000).expect("valid")
        );
        assert_eq!(
            DEDUCTIONS_2026.capital_income_rate,
            Rate::from_percent_millis(25_000).expect("valid")
        );
    }

    /// The flat rate must sit strictly between the tariff's entry and top rates, or the
    /// § 32d Abs. 6 election would never be worth making in one direction or the other.
    #[test]
    fn the_flat_rate_lies_inside_the_tariff_range() {
        use crate::income_tax::IncomeTaxTariff;
        let tariff = IncomeTaxTariff::for_year(TaxYear::new(2026).unwrap()).expect("enacted");
        let flat = DEDUCTIONS_2026.capital_income_rate.ppm();
        // Above the 14 % Eingangssteuersatz and below the 42 % Spitzensteuersatz.
        assert!(flat > 140_000);
        assert!(flat < tariff.upper_proportional.marginal_rate.ppm());
    }

    #[test]
    fn the_sick_pay_reduction_is_four_percent() {
        assert_eq!(
            DEDUCTIONS_2026.sick_pay_reduction,
            Rate::from_percent_millis(4_000).expect("valid")
        );
    }

    #[test]
    fn only_2026_is_available_and_other_years_are_refused() {
        assert!(DeductionParameters::for_year(TaxYear::new(2026).unwrap()).is_ok());
        assert!(DeductionParameters::for_year(TaxYear::new(2025).unwrap()).is_err());
    }

    #[test]
    fn the_parameters_cite_a_primary_source() {
        let p = DEDUCTIONS_2026.provenance;
        assert!(p.legal_basis.contains("§ 10"));
        assert!(p.legal_basis.contains("§ 32 Abs. 6"));
        assert!(
            p.source_url
                .starts_with("https://www.gesetze-im-internet.de/")
        );
        assert!(p.status.is_binding_law());
    }
}
