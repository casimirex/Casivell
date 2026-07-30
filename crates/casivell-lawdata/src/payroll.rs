//! Parameters for the Lohnsteuer withholding algorithm.
//!
//! # Source
//!
//! These are the constants assigned in `MPARA` and `MZTABFB` of the BMF
//! *Programmablaufplan für die maschinelle Berechnung der Lohnsteuer 2026*
//! (Anlage 1, Stand 12.11.2025, endgültig). The PAP is the authoritative
//! algorithm for German payroll withholding — not § 32a alone, and not the annual
//! assessment. Every payroll system in Germany implements it, and it is the only
//! sensible thing to check a withholding calculation against.
//!
//! # One value that surprises everybody
//!
//! `KVSATZAN = KVZ/2/100 + 0,07` — the health insurance component of the
//! Vorsorgepauschale uses **7.0 %**, not the 7.3 % that half the general 14.6 %
//! rate would give. The Vorsorgepauschale is computed on the *ermäßigter
//! Beitragssatz* of 14.0 % (§ 243 SGB V), halved. So the allowance deliberately
//! understates the employee's actual health contribution by 0.3 % of gross.
//! [`PayrollParameters::vorsorge_health_half_rate`] therefore is not, and must not
//! be, derived from [`crate::HealthInsurance::general_rate`].
//!
//! # Tax class VI gets nothing
//!
//! Neither the Arbeitnehmer-Pauschbetrag nor the Sonderausgaben-Pauschbetrag
//! applies in class VI, and no Kinderfreibetrag applies in classes V or VI. A
//! second job is taxed from the first euro. See
//! [`PayrollParameters::employee_allowance_for`].

use casivell_core::{Money, MoneyError, Rate, TaxYear};

use crate::provenance::{DataStatus, Provenance};

/// A Lohnsteuerklasse, § 38b EStG.
///
/// The variants carry the statutory Roman numerals; `Class3` is the
/// Splittingtarif class and `Class5`/`Class6` follow the separate formula of
/// § 39b Abs. 2 Satz 7 EStG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaxClass {
    /// Class I: single, or married but permanently separated.
    Class1,
    /// Class II: single parent entitled to the Entlastungsbetrag.
    Class2,
    /// Class III: married, spouse in class V or not employed. Splittingtarif.
    Class3,
    /// Class IV: married, both spouses in class IV.
    Class4,
    /// Class V: married, spouse in class III.
    Class5,
    /// Class VI: a second or further employment.
    Class6,
}

impl TaxClass {
    /// Every class, for exhaustive iteration.
    pub const ALL: [Self; 6] = [
        Self::Class1,
        Self::Class2,
        Self::Class3,
        Self::Class4,
        Self::Class5,
        Self::Class6,
    ];

    /// `KZTAB` in the PAP: the tariff multiplier, 2 for class III and 1 otherwise.
    ///
    /// The PAP divides the taxable amount by this before applying § 32a and
    /// multiplies the resulting tax by it afterwards, which is precisely the
    /// Splittingverfahren. It also scales the Solidaritätszuschlag Freigrenze.
    #[must_use]
    pub const fn tariff_divisor(self) -> i64 {
        match self {
            Self::Class3 => 2,
            _ => 1,
        }
    }

    /// Whether this class uses the § 39b Abs. 2 Satz 7 formula instead of applying
    /// § 32a directly.
    #[must_use]
    pub const fn uses_class_five_six_formula(self) -> bool {
        matches!(self, Self::Class5 | Self::Class6)
    }

    /// Whether the Entlastungsbetrag für Alleinerziehende applies.
    #[must_use]
    pub const fn has_single_parent_relief(self) -> bool {
        matches!(self, Self::Class2)
    }
}

/// Statutory parameters for the Lohnsteuer withholding algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayrollParameters {
    /// The year these parameters apply to.
    pub year: TaxYear,

    /// `ANP`: Arbeitnehmer-Pauschbetrag, § 9a Satz 1 Nr. 1 Buchst. a EStG.
    pub employee_lump_sum: Money,
    /// `SAP`: Sonderausgaben-Pauschbetrag, § 10c EStG.
    pub special_expenses_lump_sum: Money,
    /// `EFA`: Entlastungsbetrag für Alleinerziehende, § 24b EStG. Class II only.
    pub single_parent_relief: Money,
    /// Kinderfreibetrag per child in classes I, II and III, § 32 Abs. 6 EStG.
    pub child_allowance_full: Money,
    /// Kinderfreibetrag per child in class IV — half the full amount, because both
    /// parents are taxed in class IV and each gets half.
    pub child_allowance_half: Money,

    /// `RVSATZAN`: the employee's pension contribution rate used in the
    /// Vorsorgepauschale.
    pub vorsorge_pension_rate: Rate,
    /// `AVSATZAN`: the employee's unemployment contribution rate.
    pub vorsorge_unemployment_rate: Rate,
    /// The fixed part of `KVSATZAN`: half the *reduced* GKV rate, 7.0 %.
    ///
    /// See the module documentation — this is not half of 14.6 %.
    pub vorsorge_health_half_rate: Rate,
    /// `PVSATZAN` before adjustments: half the care rate, 1.8 %.
    pub vorsorge_care_rate: Rate,
    /// `PVSATZAN` in Saxony: 2.3 %.
    pub vorsorge_care_rate_saxony: Rate,
    /// The childless surcharge added to `PVSATZAN` when `PVZ = 1`.
    pub vorsorge_care_childless_surcharge: Rate,
    /// The per-child reduction subtracted from `PVSATZAN` per `PVA` step.
    pub vorsorge_care_child_reduction: Rate,
    /// `PVA`'s maximum: reductions run from the second to the fifth child.
    pub vorsorge_care_max_reductions: u8,
    /// The § 39b Abs. 2 Satz 5 Nr. 3 Buchst. e cap on the combined unemployment
    /// and health/care component: 1 900 €.
    pub vorsorge_unemployment_health_cap: Money,

    /// `BBGRVALV`: the annual pension and unemployment contribution ceiling.
    pub ceiling_pension_unemployment_annual: Money,
    /// `BBGKVPV`: the annual health and care contribution ceiling.
    pub ceiling_health_care_annual: Money,

    /// `W1STKL5`: the first threshold of § 39b Abs. 2 Satz 7.
    pub class_five_six_threshold_1: Money,
    /// `W2STKL5`: the second threshold.
    pub class_five_six_threshold_2: Money,
    /// `W3STKL5`: the third threshold.
    pub class_five_six_threshold_3: Money,
    /// The minimum rate in classes V and VI: 14 %.
    pub class_five_six_min_rate: Rate,
    /// The 42 % rate applied above the thresholds.
    pub class_five_six_upper_rate: Rate,
    /// The 45 % rate applied above the third threshold.
    pub class_five_six_top_rate: Rate,

    /// Citation.
    pub provenance: Provenance,
}

impl PayrollParameters {
    /// Returns the parameters for `year`.
    ///
    /// # Errors
    ///
    /// [`MoneyError::YearOutOfRange`] if no verified set exists. Only 2026 is
    /// available: the PAP is reissued annually and the 2025 edition has not been
    /// transcribed, so asking for 2025 is refused rather than answered with 2026's
    /// figures.
    pub const fn for_year(year: TaxYear) -> Result<Self, MoneyError> {
        match year.get() {
            2026 => Ok(PAYROLL_2026),
            other => Err(MoneyError::YearOutOfRange { year: other }),
        }
    }

    /// The Arbeitnehmer-Pauschbetrag available in `class`.
    ///
    /// Zero in class VI: a second job carries no Werbungskosten-Pauschbetrag,
    /// because the first job already used it.
    #[must_use]
    pub const fn employee_allowance_for(&self, class: TaxClass) -> Money {
        match class {
            TaxClass::Class6 => Money::ZERO,
            _ => self.employee_lump_sum,
        }
    }

    /// The Sonderausgaben-Pauschbetrag available in `class`. Zero in class VI.
    #[must_use]
    pub const fn special_expenses_allowance_for(&self, class: TaxClass) -> Money {
        match class {
            TaxClass::Class6 => Money::ZERO,
            _ => self.special_expenses_lump_sum,
        }
    }

    /// The Kinderfreibetrag per child in `class`.
    ///
    /// Classes V and VI get none: the Freibetrag is administered through the
    /// spouse's class III or the first employment's class. It affects only the
    /// Solidaritätszuschlag and church tax base, never the Lohnsteuer itself.
    #[must_use]
    pub const fn child_allowance_for(&self, class: TaxClass) -> Money {
        match class {
            TaxClass::Class1 | TaxClass::Class2 | TaxClass::Class3 => self.child_allowance_full,
            TaxClass::Class4 => self.child_allowance_half,
            TaxClass::Class5 | TaxClass::Class6 => Money::ZERO,
        }
    }
}

/// Builds a [`Money`] from whole euro inside a `const` table.
///
/// See [`crate::social`] for why the error arm resolves to zero rather than
/// panicking, and which test guards it.
const fn euro(whole: i64) -> Money {
    match Money::from_euro(whole) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

/// Builds a [`Money`] from euro and cents inside a `const` table.
const fn euro_cents(whole: i64, cents: u8) -> Money {
    match Money::from_euro_cents(whole, cents) {
        Ok(m) => m,
        Err(_) => Money::ZERO,
    }
}

/// Builds a [`Rate`] from thousandths of a percent inside a `const` table.
const fn pct_milli(percent_millis: i64) -> Rate {
    match Rate::from_percent_millis(percent_millis) {
        Ok(r) => r,
        Err(_) => Rate::ZERO,
    }
}

const fn year(value: u16) -> TaxYear {
    match TaxYear::new(value) {
        Ok(y) => y,
        Err(_) => TaxYear::MAX,
    }
}

/// PAP 2026 parameters, transcribed from Anlage 1, `MPARA` and `MZTABFB`.
const PAYROLL_2026: PayrollParameters = PayrollParameters {
    year: year(2026),

    employee_lump_sum: euro(1_230),
    special_expenses_lump_sum: euro(36),
    single_parent_relief: euro(4_260),
    child_allowance_full: euro(9_756),
    child_allowance_half: euro(4_878),

    vorsorge_pension_rate: pct_milli(9_300), // RVSATZAN = 0,0930
    vorsorge_unemployment_rate: pct_milli(1_300), // AVSATZAN = 0,0130
    vorsorge_health_half_rate: pct_milli(7_000), // the 0,07 in KVSATZAN
    vorsorge_care_rate: pct_milli(1_800),    // PVSATZAN = 0,018
    vorsorge_care_rate_saxony: pct_milli(2_300), // PVSATZAN = 0,023
    vorsorge_care_childless_surcharge: pct_milli(600), // + 0,006
    vorsorge_care_child_reduction: pct_milli(250), // − PVA * 0,0025
    vorsorge_care_max_reductions: 4,
    vorsorge_unemployment_health_cap: euro(1_900),

    ceiling_pension_unemployment_annual: euro(101_400), // BBGRVALV
    ceiling_health_care_annual: euro_cents(69_750, 0),  // BBGKVPV

    class_five_six_threshold_1: euro(14_071),  // W1STKL5
    class_five_six_threshold_2: euro(34_939),  // W2STKL5
    class_five_six_threshold_3: euro(222_260), // W3STKL5
    class_five_six_min_rate: pct_milli(14_000),
    class_five_six_upper_rate: pct_milli(42_000),
    class_five_six_top_rate: pct_milli(45_000),

    provenance: Provenance::new(
        "§ 39b EStG i. V. m. BMF-Programmablaufplan Lohnsteuer 2026, Anlage 1 (Stand 12.11.2025)",
        "https://www.gesetze-im-internet.de/estg/__39b.html",
        "2026-07-30",
        DataStatus::Enacted,
    ),
};

#[cfg(test)]
mod tests {
    use super::{PAYROLL_2026, PayrollParameters, TaxClass};
    use crate::social::SocialParameters;
    use casivell_core::{Money, Rate, TaxYear};

    #[test]
    fn no_amount_or_rate_in_the_table_is_accidentally_zero() {
        let p = PAYROLL_2026;
        let amounts = [
            ("ANP", p.employee_lump_sum),
            ("SAP", p.special_expenses_lump_sum),
            ("EFA", p.single_parent_relief),
            ("KFB full", p.child_allowance_full),
            ("KFB half", p.child_allowance_half),
            ("VSP cap", p.vorsorge_unemployment_health_cap),
            ("BBGRVALV", p.ceiling_pension_unemployment_annual),
            ("BBGKVPV", p.ceiling_health_care_annual),
            ("W1STKL5", p.class_five_six_threshold_1),
            ("W2STKL5", p.class_five_six_threshold_2),
            ("W3STKL5", p.class_five_six_threshold_3),
        ];
        for (name, amount) in amounts {
            assert!(
                !amount.is_zero() && !amount.is_negative(),
                "{name} is {} cents, so the literal was rejected",
                amount.cents()
            );
        }
        let rates = [
            ("RVSATZAN", p.vorsorge_pension_rate),
            ("AVSATZAN", p.vorsorge_unemployment_rate),
            ("KV half", p.vorsorge_health_half_rate),
            ("PVSATZAN", p.vorsorge_care_rate),
            ("PVSATZAN Saxony", p.vorsorge_care_rate_saxony),
            ("childless", p.vorsorge_care_childless_surcharge),
            ("child reduction", p.vorsorge_care_child_reduction),
            ("min rate", p.class_five_six_min_rate),
            ("upper rate", p.class_five_six_upper_rate),
            ("top rate", p.class_five_six_top_rate),
        ];
        for (name, rate) in rates {
            assert!(
                !rate.is_zero(),
                "{name} is zero, so the literal was rejected"
            );
        }
    }

    /// The PAP's annual ceilings must equal twelve times the monthly figures the
    /// SVBezGrV sets. The two are transcribed from *different documents*, so
    /// agreement is a genuine cross-source check on both — the strongest kind of
    /// consistency test available for statutory data.
    #[test]
    fn the_pap_ceilings_agree_with_the_svbezgrv_monthly_figures() {
        let payroll = PAYROLL_2026;
        let social = SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();

        let pension_annual = social.pension.ceiling_monthly.mul_int(12).unwrap();
        assert_eq!(
            payroll.ceiling_pension_unemployment_annual, pension_annual,
            "BBGRVALV disagrees with 12 x the SVBezGrV monthly pension ceiling"
        );

        let health_annual = social.health.ceiling_monthly.mul_int(12).unwrap();
        assert_eq!(
            payroll.ceiling_health_care_annual, health_annual,
            "BBGKVPV disagrees with 12 x the SVBezGrV monthly health ceiling"
        );

        // The unemployment ceiling is the pension one, so this holds too.
        let unemployment_annual = social.unemployment.ceiling_monthly.mul_int(12).unwrap();
        assert_eq!(
            payroll.ceiling_pension_unemployment_annual,
            unemployment_annual
        );
    }

    /// The Vorsorgepauschale's pension rate is the employee's actual half share, so
    /// it must equal half the SGB VI combined rate.
    #[test]
    fn the_vorsorgepauschale_pension_rate_is_half_the_statutory_rate() {
        let social = SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();
        assert_eq!(
            PAYROLL_2026.vorsorge_pension_rate,
            social.pension.contribution_rate.half().unwrap()
        );
    }

    /// The health component, by contrast, is deliberately **not** half the general
    /// GKV rate: the Vorsorgepauschale uses the reduced 14.0 % rate, giving 7.0 %
    /// rather than 7.3 %. Deriving it from `HealthInsurance::general_rate` would
    /// overstate the allowance by 0.3 % of gross, so this pins the divergence.
    #[test]
    fn the_vorsorgepauschale_health_rate_is_not_half_the_general_gkv_rate() {
        let social = SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let half_general = social.health.general_rate.half().unwrap();
        assert_eq!(half_general, Rate::from_percent_millis(7_300).unwrap());
        assert_eq!(
            PAYROLL_2026.vorsorge_health_half_rate,
            Rate::from_percent_millis(7_000).unwrap()
        );
        assert_ne!(
            PAYROLL_2026.vorsorge_health_half_rate, half_general,
            "the Vorsorgepauschale health rate must stay independent of the general rate"
        );
        // The gap is exactly 0.3 percentage points.
        assert_eq!(
            half_general
                .sub(PAYROLL_2026.vorsorge_health_half_rate)
                .unwrap(),
            Rate::from_percent_millis(300).unwrap()
        );
    }

    /// The care component of the Vorsorgepauschale mirrors the real contribution
    /// rates from SGB XI, unlike the health component.
    #[test]
    fn the_vorsorgepauschale_care_rates_match_sgb_xi() {
        let social = SocialParameters::for_year(TaxYear::new(2026).unwrap()).unwrap();
        let care = social.care;
        assert_eq!(
            PAYROLL_2026.vorsorge_care_rate,
            care.base_rate.half().unwrap()
        );
        assert_eq!(
            PAYROLL_2026.vorsorge_care_rate_saxony,
            care.base_rate
                .half()
                .unwrap()
                .add(care.saxony_employee_surcharge)
                .unwrap()
        );
        assert_eq!(
            PAYROLL_2026.vorsorge_care_childless_surcharge,
            care.childless_surcharge
        );
        assert_eq!(
            PAYROLL_2026.vorsorge_care_child_reduction,
            care.per_child_reduction
        );
        // PVA runs 0..=4, matching children two through five.
        assert_eq!(
            PAYROLL_2026.vorsorge_care_max_reductions,
            care.max_reduced_child_ordinal - 1
        );
    }

    /// Class III doubles the tariff; every other class does not.
    #[test]
    fn only_class_three_uses_the_splitting_divisor() {
        for class in TaxClass::ALL {
            let expected = if class == TaxClass::Class3 { 2 } else { 1 };
            assert_eq!(
                class.tariff_divisor(),
                expected,
                "wrong divisor for {class:?}"
            );
        }
    }

    #[test]
    fn only_classes_five_and_six_use_the_special_formula() {
        for class in TaxClass::ALL {
            let expected = matches!(class, TaxClass::Class5 | TaxClass::Class6);
            assert_eq!(
                class.uses_class_five_six_formula(),
                expected,
                "wrong formula selection for {class:?}"
            );
        }
        // Exactly two of the six.
        let count = TaxClass::ALL
            .iter()
            .filter(|c| c.uses_class_five_six_formula())
            .count();
        assert_eq!(count, 2);
    }

    /// Class VI gets neither lump sum, and classes V and VI get no
    /// Kinderfreibetrag. These are the exclusions that make a second job taxed
    /// from the first euro.
    #[test]
    fn class_six_receives_no_lump_sums() {
        let p = PAYROLL_2026;
        for class in TaxClass::ALL {
            let anp = p.employee_allowance_for(class);
            let sap = p.special_expenses_allowance_for(class);
            if class == TaxClass::Class6 {
                assert_eq!(anp, Money::ZERO, "class VI must get no ANP");
                assert_eq!(sap, Money::ZERO, "class VI must get no SAP");
            } else {
                assert_eq!(anp, p.employee_lump_sum);
                assert_eq!(sap, p.special_expenses_lump_sum);
            }
        }
    }

    #[test]
    fn the_child_allowance_follows_the_class() {
        let p = PAYROLL_2026;
        assert_eq!(
            p.child_allowance_for(TaxClass::Class1),
            p.child_allowance_full
        );
        assert_eq!(
            p.child_allowance_for(TaxClass::Class2),
            p.child_allowance_full
        );
        assert_eq!(
            p.child_allowance_for(TaxClass::Class3),
            p.child_allowance_full
        );
        assert_eq!(
            p.child_allowance_for(TaxClass::Class4),
            p.child_allowance_half
        );
        assert_eq!(p.child_allowance_for(TaxClass::Class5), Money::ZERO);
        assert_eq!(p.child_allowance_for(TaxClass::Class6), Money::ZERO);
        // The class IV amount is exactly half the full one: both parents share it.
        assert_eq!(
            p.child_allowance_half.mul_int(2).unwrap(),
            p.child_allowance_full
        );
    }

    /// The § 39b Abs. 2 Satz 7 thresholds must be strictly increasing, or the
    /// class V/VI ladder would be incoherent.
    #[test]
    fn the_class_five_six_thresholds_increase() {
        let p = PAYROLL_2026;
        assert!(p.class_five_six_threshold_1 < p.class_five_six_threshold_2);
        assert!(p.class_five_six_threshold_2 < p.class_five_six_threshold_3);
        assert!(p.class_five_six_min_rate.ppm() < p.class_five_six_upper_rate.ppm());
        assert!(p.class_five_six_upper_rate.ppm() < p.class_five_six_top_rate.ppm());
    }

    /// The class V/VI rates must equal the § 32a marginal rates they mirror.
    #[test]
    fn the_class_five_six_rates_match_the_tariff_rates() {
        use crate::income_tax::IncomeTaxTariff;
        let tariff = IncomeTaxTariff::for_year(TaxYear::new(2026).unwrap()).unwrap();
        assert_eq!(
            PAYROLL_2026.class_five_six_upper_rate,
            tariff.upper_proportional.marginal_rate
        );
        assert_eq!(
            PAYROLL_2026.class_five_six_top_rate,
            tariff.top_proportional.marginal_rate
        );
    }

    #[test]
    fn only_2026_is_available_and_other_years_are_refused() {
        assert!(PayrollParameters::for_year(TaxYear::new(2026).unwrap()).is_ok());
        // 2025's PAP has not been transcribed, so it must be refused rather than
        // silently answered with 2026 figures.
        assert!(PayrollParameters::for_year(TaxYear::new(2025).unwrap()).is_err());
    }

    #[test]
    fn the_parameters_cite_the_pap() {
        let p = PAYROLL_2026.provenance;
        assert!(p.legal_basis.contains("§ 39b EStG"));
        assert!(p.legal_basis.contains("Programmablaufplan"));
        assert!(p.status.is_binding_law());
    }
}
